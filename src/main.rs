use applevisor::*;

fn main() -> Result<()> {
    // stub call to factorial, so that linker will not strip it
    factorial(1);

    let _vm = VirtualMachine::new()?;
    let vcpu = Vcpu::new()?;

    vcpu.set_trap_debug_exceptions(true)?;
    vcpu.set_trap_debug_reg_accesses(true)?;

    // see the assembly of factorial function below
    let factorial_func: &[u32] = &[
        0x7100041f, //
        0x1a9f8408, //
        0x7100081f, //
        0x54000062, //
        0x52800020, //
        0xd65f03c0, //
        0x52800049, //
        0x52800020, //
        0x1b007d20, //
        0x6b08013f, //
        0x1a89252a, //
        0x54000082, //
        0xaa0a03e9, //
        0x6b08015f, //
        0x54ffff49, //
        0xd65f03c0, //
    ];

    // because we only use branch instructions that are relative,
    // our code is PIC, we can choose start address arbitrarily
    const START_ADDRESS: u64 = 0x100000;
    let mut mem = Mapping::new(0x100).unwrap();
    mem.map(START_ADDRESS, MemPerms::RWX)?;
    vcpu.set_reg(Reg::PC, START_ADDRESS)?;

    // Jump to 0x00 after function return, it will generate exception
    vcpu.set_reg(Reg::LR, 0x00000000000)?;

    // Factorial input values 10! = 3628800
    vcpu.set_reg(Reg::X0, 10)?;

    for (idx, instruction) in factorial_func.iter().enumerate() {
        let offset = START_ADDRESS + idx as u64 * 4;
        assert_eq!(mem.write_dword(offset, *instruction)?, 4);
    }

    match vcpu.run() {
        Ok(_) => {
            // w0 is 32 low bits of x0
            let w0 = vcpu.get_reg(Reg::X0)? & 0xffff_ffff;
            assert_eq!(w0, 3_628_800);
            println!("Ok");
        }
        Err(e) => {
            println!("{:?}", e);
            println!("{}", vcpu.get_exit_info());
        }
    }

    Ok(())
}

/// 7100041f     cmp     w0, #0x1
/// 1a9f8408     csinc   w8, w0, wzr, hi
/// 7100081f     cmp     w0, #0x2
/// 54000062     b.hs    0x100004b20 <_factorial+0x18>
/// 52800020     mov     w0, #0x1                ; =1
/// d65f03c0     ret
/// 52800049     mov     w9, #0x2                ; =2
/// 52800020     mov     w0, #0x1                ; =1
/// 1b007d20     mul     w0, w9, w0
/// 6b08013f     cmp     w9, w8
/// 1a89252a     cinc    w10, w9, lo
/// 54000082     b.hs    0x100004b44 <_factorial+0x3c>
/// aa0a03e9     mov     x9, x10
/// 6b08015f     cmp     w10, w8
/// 54ffff49     b.ls    0x100004b28 <_factorial+0x20>
/// d65f03c0     ret
#[no_mangle]
#[inline(never)]
pub fn factorial(input: u32) -> u32 {
    let input = input.max(1);
    let mut fact = 1;
    for i in 2..=input {
        fact *= i;
    }
    fact
}
