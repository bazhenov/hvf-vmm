// sum.s - AArch64 assembly to calculate sum of two numbers stored in the data section
.global _main   // Export label _start to the linker

_main:
    br x10
    hvc 0xff

.align 4
    a: .dword 0xff
    b: .dword 0xff
