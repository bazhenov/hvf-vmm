## Simple Apple ARM64 Linux Hypervisor

This project is a simple hypervizor for Apple aarch64 that loads Linux kernel.
It is based on the [Hypervisor.framework](https://developer.apple.com/documentation/hypervisor).

### Features

- simulation of a virtual port to be able to get boot logs from the Linux kernel

### How to build

1. build project
   ```console
   $ cargo build
   ```
1. compile linux kernel and link it to the project as a vmlinux file is a root directory
   ```console
   $ ln -s /path/to/linux/arch/arm64/boot/Image vmlinux
   ```
1. build device tree binary
   ```console
   $ dtc -I dts -O dtb -o ./target/board.dtb board.dts
   ```
1. run the hypervisor
   ```console
    $ ./run
    ```
