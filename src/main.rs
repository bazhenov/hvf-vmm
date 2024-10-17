use applevisor::*;
use std::{fs::File, io::Read, path::Path};

// should be aligned to 2Mb boundary
// https://www.kernel.org/doc/Documentation/arm64/booting.txt
const START_ADDRESS: u64 = 0x40000000;

fn main() -> Result<()> {
    let _vm = VirtualMachine::new()?;
    let vcpu = Vcpu::new()?;

    vcpu.set_trap_debug_exceptions(true)?;
    vcpu.set_trap_debug_reg_accesses(true)?;

    let image = read_kernel_image("./vmlinux");
    println!("Image size: 0x{:0x}", image.len());

    let mut mem = Mapping::new(image.len() * 10).unwrap();
    mem.map(START_ADDRESS, MemPerms::RWX)?;

    let bytes_copied = mem.write(START_ADDRESS, &image)?;
    assert_eq!(bytes_copied, image.len());

    // Required to setup EL correctly
    // 0x3c4 to use the same stack for both EL0/EL1
    vcpu.set_reg(Reg::CPSR, 0x3c5)?;

    let mut iv_table = Mapping::new(256 * 4).unwrap();
    iv_table.map(0, MemPerms::RX)?;
    iv_table.write_dword(0x000, 0xD4000002)?; // brk #0, for trapping back to hypervisor
    iv_table.write_dword(0x400, 0xD4000002)?;

    vcpu.set_reg(Reg::LR, 0x00000000000)?;
    //  Fake dtb address
    vcpu.set_reg(Reg::X0, 0xFF000000000)?;

    // On Aarch64 image can be run from the very beginning.
    // 2nd instruction will take care of jumping to .text
    vcpu.set_reg(Reg::PC, START_ADDRESS)?;

    loop {
        match vcpu.run() {
            Ok(_) => {
                let syndrom = vcpu.get_exit_info().exception.syndrome;
                let exception_class = (syndrom >> 26) & 0b111111;

                if exception_class == 22 {
                    println!("HVC handling");
                    continue;
                } else if exception_class == 23 {
                    println!("SMC handling");
                    vcpu.set_reg(Reg::PC, vcpu.get_reg(Reg::PC)? + 4)?;
                    continue;
                }

                println!("{}", vcpu.get_exit_info());
                println!(
                    "   Syndrome EC: 0b{:06b} ({})",
                    exception_class, exception_class
                );
                println!("   Syndrome IL: 0b{:01b}", (syndrom >> 25) & 0b1);
                println!("  Syndrome ISS: 0b{:025b}", syndrom & 0x1FFF);
                println!();

                println!("{}", vcpu);

                print_register_value("LR", vcpu.get_reg(Reg::LR)?);
                print_register_value("PC", vcpu.get_reg(Reg::PC)?);
                // The ELR_ELn register is used to store the return address from an exception.
                print_register_value("ELR_EL1", vcpu.get_sys_reg(SysReg::ELR_EL1)?);

                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", vcpu.get_exit_info());
                panic!("{:?}", e);
            }
        }
    }
}

fn print_register_value(name: &'static str, value: u64) {
    println!("{:>10} rel: {:016x}", name, value - START_ADDRESS);
}

fn read_kernel_image(path: impl AsRef<Path>) -> Vec<u8> {
    let mut file = File::open(path).expect("Failed to open file");
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("Failed to read file");
    buffer
}
