// SPDX-License-Identifier: GPL-2.0
/* Bounded EFI adapter for two core-control or three S3 boundary tracepoints. */
#include <linux/atomic.h>
#include <linux/cpu.h>
#include <linux/crc32.h>
#include <linux/efi.h>
#include <linux/ktime.h>
#include <linux/module.h>
#include <linux/string.h>
#include <linux/tracepoint.h>
#include <linux/unaligned.h>
#include <linux/uuid.h>

MODULE_IMPORT_NS("EFIVAR");
#define SIZE 64
#define RESERVE 5120ULL
#define MARGIN 1024ULL
#define PER_WRITE 256ULL
#define ATTRS (EFI_VARIABLE_NON_VOLATILE | EFI_VARIABLE_BOOTSERVICE_ACCESS | EFI_VARIABLE_RUNTIME_ACCESS)
static efi_guid_t guid = EFI_GUID(0x47d95147, 0x3665, 0x4bda, 0x8d, 0xdf, 0xd4, 0x8f, 0x0e, 0x33, 0x90, 0x23);
static char *nonce, *boot_id, *mode;
module_param(nonce, charp, 0400);
module_param(boot_id, charp, 0400);
module_param(mode, charp, 0400);
static bool armed;
module_param(armed, bool, 0600);
static unsigned int limit, attempted, completed, last_phase;
static int error;
static int results[3] = { -EINPROGRESS, -EINPROGRESS, -EINPROGRESS };
static unsigned long efi_results[3];
static u64 durations[3], before_total, before_free, before_max;
static atomic_t busy = ATOMIC_INIT(0);
static efi_char16_t name[32];
static u8 record[SIZE];
static struct tracepoint *point;

static int capacity(u64 *total, u64 *free, u64 *max)
{
	efi_status_t status;
	int err = efivar_lock();
	if (err) return err;
	status = efivar_query_variable_info(ATTRS, total, free, max);
	efivar_unlock();
	return efi_status_to_err(status);
}
static void phase(void *unused, const char *action, int value, bool start)
{
	unsigned int p = 0, i, flags;
	unsigned long irq;
	u64 begin;
	efi_status_t status = EFI_SUCCESS;
	int err;
	if (!READ_ONCE(armed)) return;
	if (limit == 2) {
		if (!strcmp(action, "syscore_suspend") && !start) p = 10;
		if (!strcmp(action, "syscore_resume") && start) p = 11;
	} else {
		if (!strcmp(action, "acpi_suspend") && value == 3) p = start ? 20 : 21;
		if (!strcmp(action, "syscore_resume") && !start) p = 22;
	}
	if (!p) return;
	if (atomic_cmpxchg(&busy, 0, 1)) {
		WRITE_ONCE(error, -EBUSY); WRITE_ONCE(armed, false); return;
	}
	i = attempted;
	if (i >= limit || p != (limit == 2 ? 10 : 20) + i) {
		error = -EPROTO; WRITE_ONCE(armed, false); goto done;
	}
	attempted++;
	flags = (irqs_disabled() ? 1 : 0) | (system_state == SYSTEM_SUSPEND ? 2 : 0) | (num_online_cpus() == 1 ? 4 : 0);
	if (flags != 7) { results[i] = error = -EPERM; WRITE_ONCE(armed, false); goto done; }
	put_unaligned_le32(p, record + 12);
	put_unaligned_le32(i + 1, record + 16);
	put_unaligned_le32(flags, record + 20);
	put_unaligned_le32(crc32_le(~0U, record, 60) ^ ~0U, record + 60);
	pr_info("pm_boundary_stage: begin nonce=%s phase=%u sequence=%u context=%u\n", nonce, p, i + 1, flags);
	begin = ktime_get_mono_fast_ns();
	local_irq_save(irq);
	err = efivar_trylock();
	if (!err) {
		status = efi_query_variable_store(ATTRS, MARGIN + PER_WRITE, true);
		if (status == EFI_SUCCESS)
			status = efivar_set_variable_locked(name, &guid, ATTRS, SIZE, record, true);
		err = efi_status_to_err(status);
		efivar_unlock();
	}
	local_irq_restore(irq);
	durations[i] = ktime_get_mono_fast_ns() - begin;
	results[i] = err; efi_results[i] = status;
	if (err) { error = err; WRITE_ONCE(armed, false); }
	else { completed++; last_phase = p; if (completed == limit) WRITE_ONCE(armed, false); }
	pr_info("pm_boundary_stage: end nonce=%s phase=%u sequence=%u error=%d efi=%lu fast_clock_delta_ns=%llu\n", nonce, p, i + 1, err, status, durations[i]);
done:
	atomic_set(&busy, 0);
}
static int status_get(char *buf, const struct kernel_param *unused)
{
	u64 total = 0, free = 0, max = 0;
	int query = capacity(&total, &free, &max);
	return scnprintf(buf, PAGE_SIZE,
		"schema=efi-boundary-stage-1\nmode=%s\nnonce=%s\nboot=%s\narmed=%u\nlimit=%u\nattempted=%u\ncompleted=%u\nlast_phase=%u\nerror=%d\nresults=%d,%d,%d\nefi=%lu,%lu,%lu\nfast_clock_deltas_ns=%llu,%llu,%llu\ncapacity_query=%d\nbefore_total=%llu\nbefore_free=%llu\nbefore_max=%llu\nafter_total=%llu\nafter_free=%llu\nafter_max=%llu\nreserve=%llu\nmargin=%llu\nper_write_budget=%llu\n",
		mode, nonce, boot_id, armed, limit, attempted, completed, last_phase, error,
		results[0], results[1], results[2], efi_results[0], efi_results[1], efi_results[2],
		durations[0], durations[1], durations[2], query, before_total, before_free, before_max, total, free, max, RESERVE, MARGIN, PER_WRITE);
}
static const struct kernel_param_ops status_ops = { .get = status_get };
module_param_cb(status, &status_ops, NULL, 0400);
static int readback_get(char *buf, const struct kernel_param *unused)
{
	u8 data[SIZE];
	unsigned long size = SIZE;
	u32 attrs = 0;
	efi_status_t status;
	int i, len, err = efivar_lock();
	if (err) return err;
	status = efivar_get_variable(name, &guid, &attrs, &size, data);
	efivar_unlock();
	if (status != EFI_SUCCESS) return efi_status_to_err(status);
	if (size > SIZE) return -EOVERFLOW;
	len = scnprintf(buf, PAGE_SIZE, "attributes=%u bytes=%lu\nhex=", attrs, size);
	for (i = 0; i < size; i++) len += scnprintf(buf + len, PAGE_SIZE - len, "%02x", data[i]);
	return len + scnprintf(buf + len, PAGE_SIZE - len, "\n");
}
static const struct kernel_param_ops read_ops = { .get = readback_get };
module_param_cb(readback, &read_ops, NULL, 0400);
static void find_point(struct tracepoint *tp, void *unused)
{ if (!strcmp(tp->name, "suspend_resume")) point = tp; }
static int __init stage_init(void)
{
	char ascii[32];
	u8 old[SIZE];
	unsigned long size = SIZE;
	uuid_t boot;
	efi_status_t status;
	int i, err;
	if (!nonce || strlen(nonce) < 8 || strlen(nonce) > 15 || !boot_id || strlen(boot_id) != 36 || uuid_parse(boot_id, &boot) || !mode || armed) return -EINVAL;
	for (i = 0; nonce[i]; i++) if (!(nonce[i] >= 'a' && nonce[i] <= 'z') && !(nonce[i] >= '0' && nonce[i] <= '9') && nonce[i] != '-') return -EINVAL;
	if (!strcmp(mode, "core")) limit = 2;
	else if (!strcmp(mode, "s3")) limit = 3;
	else return -EINVAL;
	if (!efivar_is_available() || !efivar_supports_writes() || efivar_reserved_space() != RESERVE) return -EPERM;
	snprintf(ascii, sizeof(ascii), "PMS3-%s", nonce);
	for (i = 0; ascii[i]; i++) name[i] = ascii[i];
	memcpy(record, "PMS3BND2", 8);
	put_unaligned_le16(2, record + 8); put_unaligned_le16(SIZE, record + 10);
	memcpy(record + 24, boot.b, 16); memcpy(record + 40, nonce, strlen(nonce));
	err = efivar_lock(); if (err) return err;
	status = efivar_get_variable(name, &guid, NULL, &size, old); efivar_unlock();
	if (status != EFI_NOT_FOUND) return status == EFI_SUCCESS || status == EFI_BUFFER_TOO_SMALL ? -EEXIST : efi_status_to_err(status);
	err = capacity(&before_total, &before_free, &before_max); if (err) return err;
	if (before_free < RESERVE + MARGIN + PER_WRITE * limit || before_max < SIZE + (i + 1) * 2) return -ENOSPC;
	for_each_kernel_tracepoint(find_point, NULL); if (!point) return -ENOENT;
	err = tracepoint_probe_register(point, phase, NULL); if (err) return err;
	pr_info("pm_boundary_stage: ready mode=%s nonce=%s limit=%u free=%llu armed=0\n", mode, nonce, limit, before_free);
	return 0;
}
static void __exit stage_exit(void)
{
	WRITE_ONCE(armed, false);
	tracepoint_probe_unregister(point, phase, NULL); tracepoint_synchronize_unregister();
	pr_info("pm_boundary_stage: unloaded attempted=%u completed=%u error=%d; EFI record retained\n", attempted, completed, error);
}
module_init(stage_init);
module_exit(stage_exit);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Bounded non-RTC EFI breadcrumbs at core and ACPI S3 boundaries");
