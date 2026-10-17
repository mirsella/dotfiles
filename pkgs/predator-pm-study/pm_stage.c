// SPDX-License-Identifier: GPL-2.0
#include <linux/acpi.h>
#include <linux/mc146818rtc.h>
#include <linux/module.h>
#include <linux/pm-trace.h>
#include <linux/tracepoint.h>

/* RTC phase recorder. Keep ordinary pm_trace off to preserve asynchronous PM.
 * Clear the wake alarm before arming and restore the RTC after the comparison.
 */
static bool armed;
module_param(armed, bool, 0600);
static unsigned int last_action;
module_param(last_action, uint, 0444);
static int last_value;
module_param(last_value, int, 0444);
static bool last_start;
module_param(last_start, bool, 0444);
static unsigned long markers;
module_param(markers, ulong, 0444);

static const char *const actions[] = {
	"sync_filesystems", "freeze_processes", "dpm_prepare", "dpm_suspend",
	"dpm_suspend_late", "dpm_suspend_noirq", "CPU_OFF", "syscore_suspend",
	"machine_suspend", "acpi_suspend", "syscore_resume", "CPU_ON",
	"dpm_resume_noirq", "dpm_resume_early", "dpm_resume", "dpm_complete",
	"console_resume_all", "suspend_enter",
};
static struct tracepoint *pm_tracepoint;

static void record_phase(void *unused, const char *action, int value, bool start)
{
	struct {
		u16 line;
		const char *file;
	} __packed trace_data = { .file = "predator-pm-stage" };
	unsigned long flags;
	unsigned int i;
	u8 century;

	if (!READ_ONCE(armed))
		return;
	for (i = 0; i < ARRAY_SIZE(actions); i++)
		if (!strcmp(action, actions[i]))
			break;
	if (i == ARRAY_SIZE(actions))
		return;
	trace_data.line = i + 1;
	generate_pm_trace(&trace_data, (start ? 8 : 0) | (value & 7));

	/* Preserve encoded year digits while correcting the generated 19xx century. */
	spin_lock_irqsave(&rtc_lock, flags);
	century = 20;
	if (!(CMOS_READ(RTC_CONTROL) & RTC_DM_BINARY) || RTC_ALWAYS_BCD)
		century = bin2bcd(century);
	CMOS_WRITE(century, acpi_gbl_FADT.century);
	spin_unlock_irqrestore(&rtc_lock, flags);
	WRITE_ONCE(last_action, i + 1);
	WRITE_ONCE(last_value, value);
	WRITE_ONCE(last_start, start);
	markers++;
}

static void find_pm_tracepoint(struct tracepoint *tp, void *unused)
{
	if (!strcmp(tp->name, "suspend_resume"))
		pm_tracepoint = tp;
}

static int __init pm_stage_init(void)
{
	int error;
	if (acpi_gbl_FADT.header.revision < 2 || !acpi_gbl_FADT.century)
		return -ENODEV;
	for_each_kernel_tracepoint(find_pm_tracepoint, NULL);
	if (!pm_tracepoint)
		return -ENOENT;
	error = tracepoint_probe_register(pm_tracepoint, record_phase, NULL);
	if (error)
		return error;
	pr_info("pm_stage: phase recorder loaded, armed=%d, century register=%#x\n",
		armed, acpi_gbl_FADT.century);
	return 0;
}

static void __exit pm_stage_exit(void)
{
	tracepoint_probe_unregister(pm_tracepoint, record_phase, NULL);
	tracepoint_synchronize_unregister();
	pr_info("pm_stage: unloaded after %lu RTC phase markers\n", markers);
}
module_init(pm_stage_init);
module_exit(pm_stage_exit);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Predator suspend-phase RTC recorder with corrected century");
