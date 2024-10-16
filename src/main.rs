use applevisor::*;
use std::{fs::File, io::Read, path::Path};

// should be aligned to 2Mb boundary
// https://www.kernel.org/doc/Documentation/arm64/booting.txt
const START_ADDRESS: u64 = 0x200000;

fn main() -> Result<()> {
    let _vm = VirtualMachine::new()?;
    let vcpu = Vcpu::new()?;

    vcpu.set_trap_debug_exceptions(true)?;
    vcpu.set_trap_debug_reg_accesses(true)?;

    let image = read_kernel_image("./vmlinux");

    let mut mem = Mapping::new(image.len()).unwrap();
    mem.map(START_ADDRESS, MemPerms::RX)?;

    mem.write(START_ADDRESS, &image)?;
    let mut stack = Mapping::new(0x100000).unwrap();
    stack.map(0x100000, MemPerms::RW)?;
    vcpu.set_sys_reg(SysReg::SP_EL0, 0x100000)?;
    vcpu.set_sys_reg(SysReg::SP_EL1, 0x100000)?;

    // Required to setup EL correctly
    vcpu.set_reg(Reg::CPSR, 0x3c4)?;

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

    match vcpu.run() {
        Ok(_) => {
            println!("{}", vcpu.get_exit_info());

            print_register_value("LR", vcpu.get_reg(Reg::LR)?);
            print_register_value("PC", vcpu.get_reg(Reg::PC)?);
            print_register_value("FP", vcpu.get_reg(Reg::FP)?);
            print_register_value("X0", vcpu.get_reg(Reg::X0)?);
            print_register_value("X1", vcpu.get_reg(Reg::X1)?);
            print_register_value("X2", vcpu.get_reg(Reg::X2)?);
            print_register_value("X3", vcpu.get_reg(Reg::X3)?);
            print_register_value("X4", vcpu.get_reg(Reg::X4)?);
            print_register_value("X21", vcpu.get_reg(Reg::X21)?);

            // The ELR_ELn register is used to store the return address from an exception.
            print_register_value("ELR_EL1", vcpu.get_sys_reg(SysReg::ELR_EL1)?);
            print_register_value("SP_EL0", vcpu.get_sys_reg(SysReg::SP_EL0)?);
            print_register_value("SP_EL1", vcpu.get_sys_reg(SysReg::SP_EL1)?);
            print_register_value("ELR_EL1", vcpu.get_sys_reg(SysReg::ELR_EL1)?);
            print_register_value("CPSR", vcpu.get_reg(Reg::CPSR)?);
            Ok(())
        }
        Err(e) => {
            eprintln!("{}", vcpu.get_exit_info());
            panic!("{:?}", e);
        }
    }
}

fn print_register_value(name: &'static str, value: u64) {
    println!(
        "{:>8}: 0x{:016x} (rel: 0x{:016x})",
        name,
        value,
        value - START_ADDRESS
    );
}

fn read_kernel_image(path: impl AsRef<Path>) -> Vec<u8> {
    let mut file = File::open(path).expect("Failed to open file");
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("Failed to read file");
    buffer
}
