
from mcumgr_toolkit import MCUmgrClient, mcuboot_get_image_info


import logging

from time import sleep


def main():
    FORMAT = '%(levelname)s %(name)s %(asctime)-15s %(filename)s:%(lineno)d %(message)s'
    logging.basicConfig(format=FORMAT)
    logging.getLogger().setLevel(logging.INFO)

    with MCUmgrClient.usb_serial("2fe3:.*:2") as client:
        client.use_auto_frame_size()

        data = str()
        for i in range(512):
            result = client.os_echo(data)
            print(f"{i}: {result == data}")
            data += "#"
            sleep(0.1)


if __name__ == "__main__":
    main()
