#!/usr/bin/env python3

from dbus_next.constants import BusType
from dbus_next.aio.message_bus import MessageBus
import asyncio

BLUEZ = "org.bluez"
# Replace the MAC adress with the one of your device: `bluetoothctl info`
BLUEZ_PATH = "/org/bluez/hci0/dev_EB_1A_45_AA_BD_9F"
GATT_SERVICE_INTERFACE = "org.bluez.GattService1"
GATT_CHARACTERISTIC_INTERFACE = "org.bluez.GattCharacteristic1"
GATT_DESCRIPTOR_INTERFACE = "org.bluez.GattDescriptor1"
BATTERY_SERVICE_UUID = "0000180f-0000-1000-8000-00805f9b34fb"
BATTERY_LEVEL_UUID = "00002a19-0000-1000-8000-00805f9b34fb"
BATTERY_USER_DESC_UUID = "00002901-0000-1000-8000-00805f9b34fb"

async def get_battery_info(bus):
    """Retrieve and print the battery information."""
    introspection = await bus.introspect(BLUEZ, BLUEZ_PATH)
    device = bus.get_proxy_object(BLUEZ, BLUEZ_PATH, introspection)

    for svc in device.child_paths:
        svc_introspection = await bus.introspect(BLUEZ, svc)
        svc_proxy = bus.get_proxy_object(BLUEZ, svc, svc_introspection)
        svc_interface = svc_proxy.get_interface(GATT_SERVICE_INTERFACE)

        if BATTERY_SERVICE_UUID == await svc_interface.get_uuid():
            for char in svc_proxy.child_paths:
                char_introspection = await bus.introspect(BLUEZ, char)
                char_proxy = bus.get_proxy_object(BLUEZ, char, char_introspection)
                char_interface = char_proxy.get_interface(GATT_CHARACTERISTIC_INTERFACE)

                if BATTERY_LEVEL_UUID == await char_interface.get_uuid():
                    level_bytes = await char_interface.call_read_value({})
                    battery_level = int.from_bytes(level_bytes, byteorder='little')
                    
                    # Retrieve battery user description if available
                    description = "Battery Level"
                    for desc in char_proxy.child_paths:
                        desc_introspection = await bus.introspect(BLUEZ, desc)
                        desc_proxy = bus.get_proxy_object(BLUEZ, desc, desc_introspection)
                        desc_interface = desc_proxy.get_interface(GATT_DESCRIPTOR_INTERFACE)

                        if BATTERY_USER_DESC_UUID == await desc_interface.get_uuid():
                            desc_bytes = await desc_interface.call_read_value({})
                            description = bytearray(desc_bytes).decode()

                    print(f"{description}: {battery_level}%")

async def main():
    try:
        bus = await MessageBus(bus_type=BusType.SYSTEM).connect()
        await get_battery_info(bus)
    except Exception as e:
        print(f"Error: {e}")

if __name__ == "__main__":
    asyncio.run(main())
