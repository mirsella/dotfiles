#!/usr/bin/env python3

import asyncio

from dbus_next.aio.message_bus import MessageBus
from dbus_next.constants import BusType

BLUEZ = "org.bluez"
# Replace the MAC address with the one of your device: `bluetoothctl info`
BLUEZ_PATH = "/org/bluez/hci0/dev_EB_1A_45_AA_BD_9F"
DEVICE_INTERFACE = "org.bluez.Device1"
GATT_SERVICE_INTERFACE = "org.bluez.GattService1"
GATT_CHARACTERISTIC_INTERFACE = "org.bluez.GattCharacteristic1"
GATT_DESCRIPTOR_INTERFACE = "org.bluez.GattDescriptor1"
BATTERY_SERVICE_UUID = "0000180f-0000-1000-8000-00805f9b34fb"
BATTERY_LEVEL_UUID = "00002a19-0000-1000-8000-00805f9b34fb"
BATTERY_USER_DESC_UUID = "00002901-0000-1000-8000-00805f9b34fb"


async def get_proxy(bus, path):
    introspection = await bus.introspect(BLUEZ, path)
    return bus.get_proxy_object(BLUEZ, path, introspection)


async def get_battery_description(bus, char):
    for desc_path in char.child_paths:
        descriptor = (await get_proxy(bus, desc_path)).get_interface(
            GATT_DESCRIPTOR_INTERFACE
        )
        if await descriptor.get_uuid() == BATTERY_USER_DESC_UUID:
            return bytes(await descriptor.call_read_value({})).decode()
    return "Battery Level"


async def get_battery_info(bus):
    """One status line per battery, empty when the keyboard exposes none."""
    device = await get_proxy(bus, BLUEZ_PATH)
    if not await device.get_interface(DEVICE_INTERFACE).get_connected():
        return []

    statuses = []
    for service_path in device.child_paths:
        service = await get_proxy(bus, service_path)
        service_interface = service.get_interface(GATT_SERVICE_INTERFACE)
        if await service_interface.get_uuid() != BATTERY_SERVICE_UUID:
            continue

        for char_path in service.child_paths:
            char = await get_proxy(bus, char_path)
            char_interface = char.get_interface(GATT_CHARACTERISTIC_INTERFACE)
            if await char_interface.get_uuid() != BATTERY_LEVEL_UUID:
                continue
            level = int.from_bytes(await char_interface.call_read_value({}), "little")
            description = await get_battery_description(bus, char)
            statuses.append(f"{description}: {level}%")

    return statuses


async def main():
    try:
        bus = await MessageBus(bus_type=BusType.SYSTEM).connect()
        print("\n".join(await get_battery_info(bus)) or "disconnected")
    except Exception as e:
        print(f"Error: {e}")


if __name__ == "__main__":
    asyncio.run(main())
