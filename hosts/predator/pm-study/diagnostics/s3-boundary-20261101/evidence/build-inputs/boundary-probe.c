// SPDX-License-Identifier: GPL-2.0
/* Bounded kernel-API adapter for testing the EFI transport while awake.
 * It has no suspend hooks, RTC access, reset operation or automatic writes.
 * A unique, absent variable is mandatory; one explicit write is permitted.
 */
#include <linux/atomic.h>
#include <linux/ctype.h>
#include <linux/efi.h>
#include <linux/ktime.h>
#include <linux/module.h>
#include <linux/string.h>

MODULE_IMPORT_NS("EFIVAR");

static efi_guid_t guid = EFI_GUID(0x47d95147, 0x3665, 0x4bda,
				0x8d, 0xdf, 0xd4, 0x8f, 0x0e, 0x33, 0x90, 0x23);
#define ATTRS (EFI_VARIABLE_NON_VOLATILE | EFI_VARIABLE_BOOTSERVICE_ACCESS | \
	       EFI_VARIABLE_RUNTIME_ACCESS)
static char *nonce;
module_param(nonce, charp, 0400);
static char *boot_id;
module_param(boot_id, charp, 0400);
static efi_char16_t name[64];
static char payload[192];
static size_t payload_size;
static atomic_t attempted = ATOMIC_INIT(0);
static bool ready;
static int last_errno = -EINPROGRESS;
module_param(last_errno, int, 0400);
static unsigned long last_efi_status;
module_param(last_efi_status, ulong, 0400);
static unsigned long duration_ns;
module_param(duration_ns, ulong, 0400);

static bool valid_identity(const char *s, size_t min, size_t max)
{
	size_t i, len;
	if (!s)
		return false;
	len = strlen(s);
	if (len < min || len > max)
		return false;
	for (i = 0; i < len; i++)
		if (!(s[i] >= 'a' && s[i] <= 'z') &&
		    !(s[i] >= '0' && s[i] <= '9') && s[i] != '-')
			return false;
	return true;
}

static int probe_set(const char *val, const struct kernel_param *unused)
{
	unsigned long flags;
	u64 start;
	int err;
	if (!ready || !sysfs_streq(val, "1"))
		return -EINVAL;
	if (atomic_cmpxchg(&attempted, 0, 1))
		return -EALREADY;
	start = ktime_get_mono_fast_ns();
	local_irq_save(flags);
	err = efivar_trylock();
	if (!err) {
		last_efi_status = efivar_set_variable_locked(name, &guid, ATTRS,
					payload_size, payload, true);
		err = efi_status_to_err(last_efi_status);
		efivar_unlock();
	}
	local_irq_restore(flags);
	duration_ns = ktime_get_mono_fast_ns() - start;
	last_errno = err;
	pr_info("pm_boundary_probe: nonce=%s result=%d efi=%#lx duration_ns=%lu\n",
		nonce, err, last_efi_status, duration_ns);
	return err;
}
static const struct kernel_param_ops probe_ops = { .set = probe_set };
module_param_cb(probe, &probe_ops, NULL, 0200);

static int readback_get(char *buf, const struct kernel_param *unused)
{
	char data[sizeof(payload)];
	unsigned long size = sizeof(data);
	u32 attrs = 0;
	efi_status_t status;
	int err = efivar_lock();
	if (err)
		return err;
	status = efivar_get_variable(name, &guid, &attrs, &size, data);
	efivar_unlock();
	if (status != EFI_SUCCESS)
		return efi_status_to_err(status);
	if (attrs != ATTRS || size != payload_size ||
	    memcmp(data, payload, payload_size))
		return -EBADMSG;
	return scnprintf(buf, PAGE_SIZE, "attributes=%u bytes=%lu\n%.*s",
			 attrs, size, (int)size, data);
}
static const struct kernel_param_ops readback_ops = { .get = readback_get };
module_param_cb(readback, &readback_ops, NULL, 0400);

static int __init boundary_probe_init(void)
{
	char ascii[64], existing[sizeof(payload)];
	unsigned long size = sizeof(existing);
	u64 storage, remaining, max_size;
	efi_status_t status;
	int i, err;
	if (!valid_identity(nonce, 8, 32) || !valid_identity(boot_id, 36, 36) ||
	    !efivar_is_available() || !efivar_supports_writes())
		return -EINVAL;
	snprintf(ascii, sizeof(ascii), "PredatorPmProbe-%s", nonce);
	for (i = 0; ascii[i]; i++)
		name[i] = ascii[i];
	payload_size = scnprintf(payload, sizeof(payload),
		"PMBOUNDARY-PROBE-1\nnonce=%s\nboot=%s\ncontext=awake-irqs-disabled\n",
		nonce, boot_id);
	err = efivar_lock();
	if (err)
		return err;
	status = efivar_get_variable(name, &guid, NULL, &size, existing);
	if (status != EFI_NOT_FOUND) {
		efivar_unlock();
		return status == EFI_SUCCESS || status == EFI_BUFFER_TOO_SMALL ?
			-EEXIST : efi_status_to_err(status);
	}
	status = efivar_query_variable_info(ATTRS, &storage, &remaining, &max_size);
	efivar_unlock();
	if (status != EFI_SUCCESS)
		return efi_status_to_err(status);
	if (remaining < 65536 || max_size < sizeof(payload))
		return -ENOSPC;
	ready = true;
	pr_info("pm_boundary_probe: ready nonce=%s remaining=%llu max_size=%llu; no write yet\n",
		nonce, remaining, max_size);
	return 0;
}
static void __exit boundary_probe_exit(void)
{
	ready = false;
	pr_info("pm_boundary_probe: unloaded nonce=%s attempted=%d result=%d; variable retained\n",
		nonce, atomic_read(&attempted), last_errno);
}
module_init(boundary_probe_init);
module_exit(boundary_probe_exit);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("One-write awake EFI transport probe; no suspend or RTC operation");
