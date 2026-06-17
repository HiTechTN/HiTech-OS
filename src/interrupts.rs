use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use x86_64::instructions::port::Port;
use x86_64::VirtAddr;
use pic8259::ChainedPics;
use spin;
use lazy_static::lazy_static;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

fn set_idt_entry(idt: &mut InterruptDescriptorTable) {
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
    idt[InterruptIndex::Keyboard.as_usize()]
        .set_handler_fn(keyboard_interrupt_handler);
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        set_idt_entry(&mut idt);
        idt
    };
}

pub fn init_idt() {
    IDT.load();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("\n[EXCEPTION: BREAKPOINT]\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn timer_interrupt_handler(mut frame: InterruptStackFrame) {
    crate::timer::tick();

    let current_rsp = frame.stack_pointer.as_u64();
    let new_rsp = crate::scheduler::preempt_schedule(current_rsp);
    if new_rsp != current_rsp {
        unsafe {
            frame.as_mut().update(|f| f.stack_pointer = VirtAddr::new(new_rsp));
        }
    }

    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let scancode: u8 = unsafe { Port::new(0x60).read() };
    crate::keyboard::push_key(scancode);
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard = PIC_1_OFFSET + 1,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 { self as u8 }
    fn as_usize(self) -> usize { usize::from(self.as_u8()) }
}