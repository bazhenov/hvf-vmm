use applevisor::*;
use std::{
    fmt::Display,
    fs::File,
    io::Read,
    path::Path,
    thread::{self, sleep},
    time::Duration,
};

const BOOTLOADER_ADDRESS: u64 = 0x01_0000;
const DTB_ADDRESS: u64 = 0x20_0000;
const UART_ADDRESS: u64 = 0x4100_0000;
const RAM_ADDRESS: u64 = 0x8000_0000;
const RAM_SIZE: u64 = 0x4000_0000;

// should be aligned to 2Mb boundary
// https://www.kernel.org/doc/Documentation/arm64/booting.txt
const START_ADDRESS: u64 = 0x01_0000_0000;

fn main() -> Result<()> {
    let _vm = VirtualMachine::new()?;
    let vcpu = Vcpu::new()?;

    // let iv_size = 0x800u64 / 4 + 1;
    // let mut iv_table = Mapping::new(iv_size as usize * 4).unwrap();
    // iv_table.map(0, MemPerms::RX)?;
    // for i in 0..iv_size {
    //     iv_table.write_dword(i * 4, 0xD4000002)?; // hvc 0
    //     iv_table.write_dword(i * 4, 0x020000D4)?; // hvc 0
    // }
    // vcpu.set_sys_reg(SysReg::VBAR_EL1, 0)?;

    // vcpu.set_trap_debug_exceptions(true)?;
    // vcpu.set_trap_debug_reg_accesses(true)?;

    let _kernel = map_region(
        "./vmlinux",
        START_ADDRESS,
        Some(512 * 1024 * 1024),
        MemPerms::RWX,
    )?;

    let _bootloader = map_region("./main.bin", BOOTLOADER_ADDRESS, None, MemPerms::RX)?;
    let _dtb = map_region("./device-tree-block", DTB_ADDRESS, None, MemPerms::R)?;

    // Required to setup EL correctly
    // 0x3c4 to use the same stack for both EL0/EL1
    vcpu.set_reg(Reg::CPSR, 0x3c5)?;

    let mut m = Mapping::new(0x800).unwrap();
    m.map(0, MemPerms::RX).unwrap();
    for i in [0x00, 0x200, 0x400, 0x600] {
        m.write_dword(i, 0xD4001FE2)?; // hvc 255
        m.write_dword(i + 4, 0xD69F03E0)?; // eret
    }

    let mut ram = Mapping::new(RAM_SIZE as usize).unwrap();
    ram.map(RAM_ADDRESS, MemPerms::RW).unwrap();

    let mut mmio = Mmio::default();
    mmio.register(UART_ADDRESS, PAGE_SIZE as u64, pl011_uart::Controller)?;

    // By lonux boot convetions, X0 should contains the address of the device tree block
    vcpu.set_reg(Reg::X0, DTB_ADDRESS)?;

    // On Aarch64 image can be run from the very beginning.
    // 2nd instruction will take care of jumping to .text
    vcpu.set_reg(Reg::PC, BOOTLOADER_ADDRESS)?;
    vcpu.set_reg(Reg::X10, START_ADDRESS)?;

    // Disabling MMU
    vcpu.set_sys_reg(
        SysReg::SCTLR_EL1,
        vcpu.get_sys_reg(SysReg::SCTLR_EL1)? & !0x1,
    )?;

    let instance = vcpu.get_instance();
    thread::spawn(move || {
        sleep(Duration::from_secs(1));
        Vcpu::stop(&[instance]).unwrap();
    });

    loop {
        match vcpu.run() {
            Ok(_) => {
                let exit_info = vcpu.get_exit_info();
                let pc = vcpu.get_reg(Reg::PC)?;
                match exit_info.reason {
                    ExitReason::CANCELED => {
                        println!("Canceled");

                        println!("{}", vcpu);

                        print_register_value("LR", vcpu.get_reg(Reg::LR)?);
                        print_register_value("PC", vcpu.get_reg(Reg::PC)?);
                        // The ELR_ELn register is used to store the return address from an exception.
                        print_register_value("ELR_EL1", vcpu.get_sys_reg(SysReg::ELR_EL1)?);
                        break;
                    }
                    ExitReason::EXCEPTION => {
                        let syndrom = exit_info.exception.syndrome;
                        let exception_class = ExceptionClass::from(syndrom);

                        let next_pc = match exception_class {
                            ExceptionClass::HvcRequested => {
                                let imm = syndrom & 0xFFFF;
                                println!("HVC handling (call #{})", imm);
                                (imm != 0xff).then_some(pc)
                            }
                            ExceptionClass::MsrMrsTrap => {
                                print!("MSR/MRS trap: {}", vcpu.get_exit_info());
                                trap_msr_mrs(&vcpu, syndrom)?.then_some(pc + 4)
                            }
                            ExceptionClass::SmcRequested => {
                                println!("SMC handling");
                                Some(pc + 4)
                            }
                            ExceptionClass::BrkInstruction => {
                                println!("Explicit BRK instruction");
                                None
                            }
                            ExceptionClass::InstructionAbortMmuFault => {
                                println!("{}", exception_class);
                                println!();
                                None
                            }
                            ExceptionClass::DataAbortMmuFault => {
                                let physicall_addr = exit_info.exception.physical_address;
                                handle_mmu_fault(&vcpu, &mut mmio, physicall_addr, syndrom)?
                                    .then_some(pc + 4)
                            }
                            ExceptionClass::Unknown(_) => None,
                        };

                        if let Some(next_pc) = next_pc {
                            vcpu.set_reg(Reg::PC, next_pc)?;
                            continue;
                        } else {
                            println!("{}", exit_info);
                            println!("{}", vcpu);

                            println!(
                                "{} at address 0x{:x}",
                                exception_class, exit_info.exception.physical_address
                            );
                            println!(
                                "EL2 virtual address 0x{:x}",
                                exit_info.exception.virtual_address
                            );

                            println!(
                                "Pending interrupt FIQ: {}, IQR: {}",
                                vcpu.get_pending_interrupt(InterruptType::FIQ)?,
                                vcpu.get_pending_interrupt(InterruptType::IRQ)?
                            );

                            print_register_value("LR", vcpu.get_reg(Reg::LR)?);
                            print_register_value("PC", vcpu.get_reg(Reg::PC)?);
                            // The ELR_ELn register is used to store the return address from an exception.
                            print_register_value("ELR_EL1", vcpu.get_sys_reg(SysReg::ELR_EL1)?);

                            break;
                        }
                    }
                    ExitReason::VTIMER_ACTIVATED => todo!(),
                    ExitReason::UNKNOWN => todo!(),
                }
            }
            Err(e) => {
                eprintln!("{}", vcpu.get_exit_info());
                eprintln!("{:?}", e);
                break;
            }
        }
    }
    Ok(())
}

/// Returns true if MMU fault was handled successfully. In this case
/// the PC should be incremented by 4 and execution should continue.
fn handle_mmu_fault(
    vcpu: &Vcpu,
    mmio: &mut Mmio,
    physical_address: u64,
    syndrom: u64,
) -> Result<bool> {
    let iss = syndrom & 0x1FFFFFF;
    let reg = (iss >> 16) & 0b11111;
    let write = (iss >> 6) & 1;

    let Some(device) = mmio.find_controller(physical_address) else {
        return Ok(false);
    };
    let address = physical_address - device.base;

    if write == 1 {
        let value = vcpu.get_reg(get_register(reg))?;
        if device.controller.write(address, value).is_some() {
            Ok(true)
        } else {
            Ok(false)
        }
    } else if let Some(value) = device.controller.read(address) {
        vcpu.set_reg(get_register(reg), value)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[derive(PartialEq)]
struct MsrISS {
    crm: u64,
    crn: u64,
    op1: u64,
    op2: u64,
    op0: u64,
}

impl MsrISS {
    fn new(syndrom: u64) -> Self {
        Self {
            crm: (syndrom >> 1) & 0b1111,
            crn: (syndrom >> 10) & 0b1111,
            op1: (syndrom >> 14) & 0b111,
            op2: (syndrom >> 17) & 0b111,
            op0: (syndrom >> 20) & 0b11,
        }
    }
}

const ISS_MDSCR_EL1: MsrISS = MsrISS {
    crm: 0b0010,
    crn: 0b0000,
    op1: 0b000,
    op2: 0b010,
    op0: 0b10,
};

/// Process a Hypervisor traps of MSR/MRS instructions from EL1-level
///  https://developer.arm.com/documentation/ddi0601/2022-12/AArch64-Registers/ESR-EL2--Exception-Syndrome-Register--EL2-?lang=en#fieldset_0-24_0_14
fn trap_msr_mrs(vcpu: &Vcpu, syndrom: u64) -> Result<bool> {
    let iss = MsrISS::new(syndrom);
    let reg = (syndrom >> 6) & 0b11111;
    let write = syndrom & 0b1 == 0;

    if write {
        if iss == ISS_MDSCR_EL1 {
            let value = vcpu.get_reg(get_register(reg))?;
            vcpu.set_sys_reg(SysReg::MDSCR_EL1, value)?;
            return Ok(true);
        }
        println!("Write access prohibited");
        println!(
            "Op0: 0b{:02b} Op1: 0b{:03b} Op2: 0b{:03b} CRn: 0b{:04b} CRm: 0b{:04b}",
            iss.op0, iss.op1, iss.op2, iss.crn, iss.crm
        );

        Ok(false)
    } else {
        vcpu.set_reg(get_register(reg), 0)?;
        Ok(true)
    }
}

fn get_register(reg: u64) -> Reg {
    match reg {
        0 => Reg::X0,
        1 => Reg::X1,
        2 => Reg::X2,
        3 => Reg::X3,
        4 => Reg::X4,
        5 => Reg::X5,
        6 => Reg::X6,
        7 => Reg::X7,
        8 => Reg::X8,
        9 => Reg::X9,
        10 => Reg::X10,
        11 => Reg::X11,
        12 => Reg::X12,
        13 => Reg::X13,
        14 => Reg::X14,
        15 => Reg::X15,
        16 => Reg::X16,
        17 => Reg::X17,
        18 => Reg::X18,
        19 => Reg::X19,
        20 => Reg::X20,
        21 => Reg::X21,
        22 => Reg::X22,
        23 => Reg::X23,
        24 => Reg::X24,
        25 => Reg::X25,
        26 => Reg::X26,
        27 => Reg::X27,
        28 => Reg::X28,
        29 => Reg::X29,
        30 => Reg::X30,
        _ => panic!("Invalid register number"),
    }
}

fn map_region(
    path: &str,
    addr: u64,
    size: Option<usize>,
    permissions: MemPerms,
) -> Result<Mapping> {
    let image = read_image(path);
    let mut mem = Mapping::new(size.unwrap_or(image.len())).unwrap();
    mem.map(addr, permissions)?;
    let bytes_copied = mem.write(addr, &image)?;
    assert_eq!(bytes_copied, image.len());
    Ok(mem)
}

fn print_register_value(name: &'static str, value: u64) {
    println!("{:>10} rel: {:016x}", name, value - START_ADDRESS);
}

fn read_image(path: impl AsRef<Path>) -> Vec<u8> {
    let mut file = File::open(path).expect("Failed to open file");
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("Failed to read file");
    buffer
}

enum ExceptionClass {
    HvcRequested,
    SmcRequested,
    MsrMrsTrap,
    BrkInstruction,
    InstructionAbortMmuFault,
    DataAbortMmuFault,
    Unknown(u64),
}

impl From<u64> for ExceptionClass {
    fn from(value: u64) -> Self {
        match (value >> 26) & 0b111111 {
            22 => Self::HvcRequested,
            24 => Self::MsrMrsTrap,
            23 => Self::SmcRequested,
            60 => Self::BrkInstruction,
            32 => Self::InstructionAbortMmuFault,
            36 => Self::DataAbortMmuFault,
            c => Self::Unknown(c),
        }
    }
}

impl Display for ExceptionClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::HvcRequested => "HVC requested",
            Self::SmcRequested => "SMC requested",
            Self::BrkInstruction => "Explicit BRK instruction",
            Self::InstructionAbortMmuFault => "Instruction Abort MMU fault",
            Self::DataAbortMmuFault => "Data Abort MMU fault",
            Self::MsrMrsTrap => "MSR/MRS trap",
            Self::Unknown(_) => "Unknown",
        };
        f.write_str(message)
    }
}

#[derive(Default)]
struct Mmio {
    periphery: Vec<MmioPeriphery>,
}

impl Mmio {
    pub fn register(
        &mut self,
        base: u64,
        size: u64,
        controller: impl MmioController + 'static,
    ) -> Result<()> {
        let mut mapping = Mapping::new(size as usize).unwrap();
        mapping.map(base, MemPerms::None)?;
        let controller = Box::new(controller);
        self.periphery.push(MmioPeriphery {
            base,
            size,
            controller,
            mapping,
        });
        Ok(())
    }

    pub fn find_controller(&mut self, addr: u64) -> Option<&mut MmioPeriphery> {
        // TODO: here we need to check different read/write lengths
        self.periphery
            .iter_mut()
            .find(|p| addr >= p.base && addr < p.base + p.size)
    }
}

struct MmioPeriphery {
    base: u64,
    size: u64,
    controller: Box<dyn MmioController>,
    // We need to store mapping, otherwise it will be unmapped from guest memory
    mapping: Mapping,
}

trait MmioController {
    fn read(&self, addr: u64) -> Option<u64>;
    fn write(&mut self, addr: u64, value: u64) -> Option<u64>;
}

mod pl011_uart {
    use super::*;

    const UART01x_FR: u64 = 0x18;
    const UARTDR: u64 = 0x00;

    pub struct Controller;

    impl MmioController for Controller {
        fn read(&self, addr: u64) -> Option<u64> {
            match addr {
                UART01x_FR => Some(0),
                _ => None,
            }
        }

        fn write(&mut self, addr: u64, value: u64) -> Option<u64> {
            match addr {
                UARTDR => {
                    if let Some(ch) = char::from_u32(value as u32) {
                        print!("{}", ch);
                    } else {
                        println!("Non ASCII: 0x{:x}", value);
                    }
                    Some(value)
                }
                _ => None,
            }
        }
    }
}
