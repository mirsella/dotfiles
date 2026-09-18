// SPDX-License-Identifier: GPL-2.0-only
/*
 * Throwaway probe: call the Acer Predator gaming WMI interface
 * (GUID 7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56) with an arbitrary u64
 * payload, to find the keyboard-backlight encoding of method 2.
 * NOT for upstream. Load with e.g.:
 *   insmod wmi-led-test.ko method=2 value=65537
 */
#include <linux/acpi.h>
#include <linux/module.h>
#include <linux/wmi.h>

#define ACER_GAMING_GUID "7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56"

static unsigned int method = 2;
module_param(method, uint, 0444);
MODULE_PARM_DESC(method, "WMI method id (2=set LED, 4=get LED)");

static unsigned long long value;
module_param(value, ullong, 0444);
MODULE_PARM_DESC(value, "u64 input payload");

static int __init ledtest_init(void)
{
	struct acpi_buffer input = { sizeof(value), &value };
	struct acpi_buffer output = { ACPI_ALLOCATE_BUFFER, NULL };
	union acpi_object *obj;
	acpi_status status;

	status = wmi_evaluate_method(ACER_GAMING_GUID, 0, method, &input,
				     &output);
	pr_info("wmi-led-test: method %u in 0x%llx -> status %u\n", method,
		value, status);
	if (ACPI_FAILURE(status))
		return 0;

	obj = output.pointer;
	if (!obj) {
		pr_info("wmi-led-test: no output object\n");
	} else if (obj->type == ACPI_TYPE_INTEGER) {
		pr_info("wmi-led-test: out INTEGER 0x%llx\n",
			obj->integer.value);
	} else if (obj->type == ACPI_TYPE_BUFFER) {
		u32 first = 0;
		u64 full = 0;

		if (obj->buffer.length >= 4)
			memcpy(&first, obj->buffer.pointer, 4);
		if (obj->buffer.length >= 8)
			memcpy(&full, obj->buffer.pointer, 8);
		pr_info("wmi-led-test: out BUFFER len %u first32 0x%x first64 0x%llx\n",
			obj->buffer.length, first, full);
	} else {
		pr_info("wmi-led-test: out type %u\n", obj->type);
	}
	kfree(output.pointer);
	return 0;
}

static void __exit ledtest_exit(void)
{
}

module_init(ledtest_init);
module_exit(ledtest_exit);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Throwaway Acer gaming-WMI LED probe");
