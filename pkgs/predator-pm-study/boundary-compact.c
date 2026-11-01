// SPDX-License-Identifier: GPL-2.0
/* One awake EFI breadcrumb write. No suspend hooks, RTC access or erase path. */
#include <linux/atomic.h>
#include <linux/crc32.h>
#include <linux/efi.h>
#include <linux/ktime.h>
#include <linux/module.h>
#include <linux/string.h>
#include <linux/unaligned.h>
#include <linux/uuid.h>

MODULE_IMPORT_NS("EFIVAR");

#define RECORD_SIZE 64
#define WRITE_BUDGET 1024ULL
#define EXTRA_MARGIN 1024ULL
#define EXPECTED_RESERVE 5120ULL
#define ATTRS (EFI_VARIABLE_NON_VOLATILE | EFI_VARIABLE_BOOTSERVICE_ACCESS | \
	       EFI_VARIABLE_RUNTIME_ACCESS)

static efi_guid_t guid = EFI_GUID(0x47d95147, 0x3665, 0x4bda,
				0x8d, 0xdf, 0xd4, 0x8f, 0x0e, 0x33, 0x90, 0x23);
static char *nonce, *boot_id;
module_param(nonce, charp, 0400);
module_param(boot_id, charp, 0400);
static efi_char16_t name[32];
static u8 record[RECORD_SIZE];
static unsigned int charged_bytes;
static atomic_t attempted = ATOMIC_INIT(0);
static bool ready;
static int write_errno = -EINPROGRESS, audit_errno = -EINPROGRESS;
static efi_status_t write_status;
static u64 before_storage, before_remaining, before_max;
static u64 after_storage, after_remaining, after_max, duration_ns;

static bool valid_nonce(const char *s)
{
	size_t i, len;
	if (!s)
		return false;
	len = strlen(s);
	if (len < 8 || len > 15)
		return false;
	for (i = 0; i < len; i++)
		if (!(s[i] >= 'a' && s[i] <= 'z') &&
		    !(s[i] >= '0' && s[i] <= '9') && s[i] != '-')
			return false;
	return true;
}

/* Called awake with IRQs enabled; the ordinary query may sleep. */
static int capacity(u64 *storage, u64 *remaining, u64 *max)
{
	efi_status_t status;
	int err = efivar_lock();
	if (err)
		return err;
	status = efivar_query_variable_info(ATTRS, storage, remaining, max);
	efivar_unlock();
	return efi_status_to_err(status);
}

static int check_budget(void)
{
	int err = capacity(&before_storage, &before_remaining, &before_max);
	if (err)
		return err;
	if (efivar_reserved_space() != EXPECTED_RESERVE) {
		pr_err("pm_boundary_compact: unexpected EFI reserve %llu; refusing\n",
		       efivar_reserved_space());
		return -EPERM;
	}
	if (before_max < charged_bytes ||
	    before_remaining < EXPECTED_RESERVE + EXTRA_MARGIN + WRITE_BUDGET) {
		pr_err("pm_boundary_compact: insufficient budget: free=%llu max=%llu charged=%u required_free=%llu\n",
		       before_remaining, before_max, charged_bytes,
		       EXPECTED_RESERVE + EXTRA_MARGIN + WRITE_BUDGET);
		return -ENOSPC;
	}
	return 0;
}

static int write_set(const char *val, const struct kernel_param *unused)
{
	unsigned long flags;
	u64 start;
	int err;
	if (!ready || !sysfs_streq(val, "1"))
		return -EINVAL;
	if (atomic_cmpxchg(&attempted, 0, 1))
		return -EALREADY;
	err = check_budget();
	if (err) {
		write_errno = err;
		return err;
	}
	start = ktime_get_mono_fast_ns();
	local_irq_save(flags);
	err = efivar_trylock();
	if (!err) {
		/* Read-only check of the larger budget through Linux's own reserve
		 * logic. Nonblocking deliberately avoids its GC-triggering path. */
		write_status = efi_query_variable_store(ATTRS,
				WRITE_BUDGET + EXTRA_MARGIN, true);
		if (write_status == EFI_SUCCESS)
			write_status = efivar_set_variable_locked(name, &guid, ATTRS,
						 sizeof(record), record, true);
		err = efi_status_to_err(write_status);
		efivar_unlock();
	}
	local_irq_restore(flags);
	duration_ns = ktime_get_mono_fast_ns() - start;
	write_errno = err;
	audit_errno = capacity(&after_storage, &after_remaining, &after_max);
	/* MaximumVariableSize may track remaining capacity on this firmware.
	 * Check the usable bound, not equality with its pre-write value. */
	if (!audit_errno && (after_storage != before_storage ||
			     after_remaining > before_remaining))
		audit_errno = -ESTALE;
	if (!audit_errno && (before_remaining - after_remaining > WRITE_BUDGET ||
			     after_remaining < EXPECTED_RESERVE + EXTRA_MARGIN ||
			     after_max < charged_bytes))
		audit_errno = -EOVERFLOW;
	pr_info("pm_boundary_compact: nonce=%s write_errno=%d audit_errno=%d efi=%#lx free_before=%llu free_after=%llu duration_ns=%llu; record retained\n",
		nonce, write_errno, audit_errno, write_status,
		before_remaining, after_remaining, duration_ns);
	return write_errno ? write_errno : audit_errno;
}
static const struct kernel_param_ops write_ops = { .set = write_set };
module_param_cb(write_once, &write_ops, NULL, 0200);

static int status_get(char *buf, const struct kernel_param *unused)
{
	return scnprintf(buf, PAGE_SIZE,
		"schema=compact-efi-probe-1\nattempted=%d\nwrite_errno=%d\naudit_errno=%d\nefi_status=%lu\n"
		"reserve_bytes=%llu\nmargin_bytes=%llu\nallocation_budget_bytes=%llu\ncharged_bytes=%u\n"
		"before_storage_bytes=%llu\nbefore_remaining_bytes=%llu\nbefore_max_bytes=%llu\n"
		"after_storage_bytes=%llu\nafter_remaining_bytes=%llu\nafter_max_bytes=%llu\nwrite_duration_ns=%llu\n",
		atomic_read(&attempted), write_errno, audit_errno, write_status,
		EXPECTED_RESERVE, EXTRA_MARGIN, WRITE_BUDGET, charged_bytes,
		before_storage, before_remaining, before_max,
		after_storage, after_remaining, after_max, duration_ns);
}
static const struct kernel_param_ops status_ops = { .get = status_get };
module_param_cb(status, &status_ops, NULL, 0400);

static int readback_get(char *buf, const struct kernel_param *unused)
{
	u8 data[RECORD_SIZE];
	unsigned long size = sizeof(data);
	u32 attrs = 0;
	efi_status_t status;
	int i, len, err = efivar_lock();
	if (err)
		return err;
	status = efivar_get_variable(name, &guid, &attrs, &size, data);
	efivar_unlock();
	if (status != EFI_SUCCESS)
		return efi_status_to_err(status);
	if (attrs != ATTRS || size != sizeof(record) || memcmp(data, record, size))
		return -EBADMSG;
	len = scnprintf(buf, PAGE_SIZE, "attributes=%u bytes=%lu\nhex=", attrs, size);
	for (i = 0; i < size; i++)
		len += scnprintf(buf + len, PAGE_SIZE - len, "%02x", data[i]);
	return len + scnprintf(buf + len, PAGE_SIZE - len, "\n");
}
static const struct kernel_param_ops readback_ops = { .get = readback_get };
module_param_cb(readback, &readback_ops, NULL, 0400);

static int __init compact_init(void)
{
	char ascii[32];
	u8 existing[RECORD_SIZE];
	unsigned long size = sizeof(existing);
	uuid_t boot;
	efi_status_t status;
	int i, err;
	if (!valid_nonce(nonce) || !boot_id || strlen(boot_id) != 36 ||
	    uuid_parse(boot_id, &boot) || !efivar_is_available() || !efivar_supports_writes())
		return -EINVAL;
	snprintf(ascii, sizeof(ascii), "PMS3-%s", nonce);
	for (i = 0; ascii[i]; i++)
		name[i] = ascii[i];
	charged_bytes = sizeof(record) + (i + 1) * sizeof(*name);
	memcpy(record, "PMS3BND2", 8);
	put_unaligned_le16(2, record + 8);
	put_unaligned_le16(RECORD_SIZE, record + 10);
	put_unaligned_le32(1, record + 12); /* phase 1: awake transport probe */
	put_unaligned_le32(1, record + 16); /* exactly one write */
	memcpy(record + 24, boot.b, 16);
	memcpy(record + 40, nonce, strlen(nonce));
	put_unaligned_le32(crc32_le(~0U, record, 60) ^ ~0U, record + 60);
	err = efivar_lock();
	if (err)
		return err;
	status = efivar_get_variable(name, &guid, NULL, &size, existing);
	efivar_unlock();
	if (status != EFI_NOT_FOUND)
		return status == EFI_SUCCESS || status == EFI_BUFFER_TOO_SMALL ?
			-EEXIST : efi_status_to_err(status);
	err = check_budget();
	if (err)
		return err;
	ready = true;
	pr_info("pm_boundary_compact: ready nonce=%s record=%zu charged=%u free=%llu; no write yet\n",
		nonce, sizeof(record), charged_bytes, before_remaining);
	return 0;
}
static void __exit compact_exit(void)
{
	ready = false;
	pr_info("pm_boundary_compact: unloaded attempted=%d write_errno=%d audit_errno=%d; no erase\n",
		atomic_read(&attempted), write_errno, audit_errno);
}
module_init(compact_init);
module_exit(compact_exit);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("One 64-byte awake EFI breadcrumb with measured allocation budget");
