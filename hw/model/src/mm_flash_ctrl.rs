// Licensed under the Apache-2.0 license

use registers_generated::mci;
use registers_generated::mci::bits::MboxExecute;
use registers_generated::mci::bits::MboxTargetStatus;
use romtime::StaticRef;
use std::fs::{File, OpenOptions};
use std::io::{Read, Result as IoResult, Seek, Write};
use std::path::PathBuf;
use std::sync::atomic;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tock_registers::interfaces::{Readable, Writeable};

pub const PAGE_SIZE: usize = 256;
pub const NUM_PAGES: usize = (64 * 1024 * 1024) / PAGE_SIZE; //64MB flash

/// Enum for mailbox flash operations.
#[derive(Debug, Copy, Clone, PartialEq)]
enum FlashOp {
    Read,
    Write,
    Erase,
    Unknown,
}

impl From<u32> for FlashOp {
    fn from(cmd: u32) -> Self {
        match cmd {
            1 => FlashOp::Read,
            2 => FlashOp::Write,
            3 => FlashOp::Erase,
            _ => FlashOp::Unknown,
        }
    }
}

fn initialize_flash_file(
    file: &mut File,
    size: usize,
    initial_content: Option<&[u8]>,
) -> IoResult<()> {
    let mut remaining = size;
    if let Some(content) = initial_content {
        let write_size = std::cmp::min(size, content.len());
        file.write_all(&content[..write_size])?;
        remaining -= write_size;
    }
    let chunk = vec![0xff; 1048576]; // 1MB chunk
    while remaining > 0 {
        let write_size = std::cmp::min(remaining, chunk.len());
        file.write_all(&chunk[..write_size])?;
        remaining -= write_size;
    }
    Ok(())
}

pub struct ImaginaryFlashController {
    mci: StaticRef<mci::regs::Mci>,
    flash_file: Arc<Mutex<File>>,
    busy: atomic::AtomicBool,
    //soc_agent: MciMailboxRequester,
}

impl ImaginaryFlashController {
    pub fn new(
        mci: StaticRef<mci::regs::Mci>,
        file_name: Option<PathBuf>,
        initial_content: Option<&[u8]>,
    ) -> Self {
        let path = file_name.unwrap_or_else(|| PathBuf::from("emulator_mm_flash.bin"));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .expect("Failed to open flash file");

        let capacity = NUM_PAGES * PAGE_SIZE;
        let metadata = file.metadata().expect("Failed to get file metadata");
        if metadata.len() < capacity as u64 || initial_content.is_some() {
            file.set_len(capacity as u64)
                .expect("Failed to set file length");
            file.seek(std::io::SeekFrom::Start(0)).unwrap();
            initialize_flash_file(&mut file, capacity, initial_content)
                .expect("Failed to init flash");
        }

        Self {
            mci,
            flash_file: Arc::new(Mutex::new(file)),
            busy: atomic::AtomicBool::new(false),
            //soc_agent: MciMailboxRequester::Mcu,
        }
    }

    pub fn poll_mailbox_and_process(&self) {
        if self.busy.load(atomic::Ordering::SeqCst) {
            // Reject or defer new command
            return;
        }

        // let execute = self.mci.mcu_mbox0_csr_mbox_execute.get();
        if self.mci.mcu_mbox0_csr_mbox_execute.get() != MboxExecute::Execute::SET.value {
            return;
        }

        self.busy.store(true, atomic::Ordering::SeqCst);

        let cmd = self.mci.mcu_mbox0_csr_mbox_cmd.get();
        // Read page number and size from SRAM offsets 0 and 1
        let page_num = self.mci.mcu_mbox0_csr_mbox_sram[0].get();
        let page_size_reg = self.mci.mcu_mbox0_csr_mbox_sram[1].get();

        let op = FlashOp::from(cmd);

        let done_bit = MboxTargetStatus::Done::SET.value;

        let status_field = match op {
            FlashOp::Read => {
                if page_num < NUM_PAGES as u32 && page_size_reg as usize == PAGE_SIZE {
                    let mut page_buf = vec![0u8; PAGE_SIZE];
                    let io_res = {
                        let mut file = self.flash_file.lock().unwrap();
                        file.seek(std::io::SeekFrom::Start(page_num as u64 * PAGE_SIZE as u64))
                            .and_then(|_| file.read_exact(&mut page_buf))
                    };
                    if io_res.is_ok() {
                        for (i, chunk) in page_buf.chunks(4).enumerate() {
                            let word = chunk
                                .iter()
                                .enumerate()
                                .fold(0u32, |acc, (j, &b)| acc | ((b as u32) << (j * 8)));
                            self.mci.mcu_mbox0_csr_mbox_sram[i].set(word);
                        }
                        self.mci.mcu_mbox0_csr_mbox_dlen.set(PAGE_SIZE as u32);
                        MboxTargetStatus::Status::CmdComplete.value
                    } else {
                        MboxTargetStatus::Status::CmdFailure.value
                    }
                } else {
                    MboxTargetStatus::Status::CmdFailure.value
                }
            }
            FlashOp::Write => {
                if page_num < NUM_PAGES as u32 && page_size_reg as usize == PAGE_SIZE {
                    let mut page_buf = vec![0u8; PAGE_SIZE];
                    for i in 0..(PAGE_SIZE / 4) {
                        let word = self.mci.mcu_mbox0_csr_mbox_sram[2 + i].get();
                        for j in 0..4 {
                            page_buf[i * 4 + j] = ((word >> (j * 8)) & 0xff) as u8;
                        }
                    }
                    let io_res = {
                        let mut file = self.flash_file.lock().unwrap();
                        file.seek(std::io::SeekFrom::Start(page_num as u64 * PAGE_SIZE as u64))
                            .and_then(|_| file.write_all(&page_buf))
                    };
                    if io_res.is_ok() {
                        MboxTargetStatus::Status::CmdComplete.value
                    } else {
                        MboxTargetStatus::Status::CmdFailure.value
                    }
                } else {
                    MboxTargetStatus::Status::CmdFailure.value
                }
            }
            FlashOp::Erase => {
                if page_num < NUM_PAGES as u32 && page_size_reg as usize == PAGE_SIZE {
                    let erase_buf = vec![0xFFu8; PAGE_SIZE];
                    let io_res = {
                        let mut file = self.flash_file.lock().unwrap();
                        file.seek(std::io::SeekFrom::Start(page_num as u64 * PAGE_SIZE as u64))
                            .and_then(|_| file.write_all(&erase_buf))
                    };
                    if io_res.is_ok() {
                        MboxTargetStatus::Status::CmdComplete.value
                    } else {
                        MboxTargetStatus::Status::CmdFailure.value
                    }
                } else {
                    MboxTargetStatus::Status::CmdFailure.value
                }
            }
            FlashOp::Unknown => MboxTargetStatus::Status::CmdFailure.value,
        };

        // Update the target status register
        self.mci
            .mcu_mbox0_csr_mbox_target_status
            .set((status_field & 0xf) | done_bit);

        println!(
            "[xs debug]poll_mailbox_and_process: cmd={} op={:?} page_num={} page_size={}, status={:#x}",
            cmd, op, page_num, page_size_reg, status_field
        );

        self.busy.store(false, atomic::Ordering::SeqCst);

        // Wait for EXECUTE bit to transition from SET to CLEAR,
        // then for a new SET (next request)
        // This avoids missing back-to-back requests.
        let mut prev_execute = MboxExecute::Execute::SET.value;
        let timeout = Duration::from_millis(100); // 100ms timeout
        let start = Instant::now();
        // Wait for transition SET -> CLEAR
        loop {
            let cur_execute = self.mci.mcu_mbox0_csr_mbox_execute.get();
            if prev_execute == MboxExecute::Execute::SET.value
                && cur_execute == MboxExecute::Execute::CLEAR.value
            {
                // Transition detected: SET -> CLEAR
                // println!("[xs debug] EXECUTE bit Clear detected");
                break;
            }
            if start.elapsed() > timeout {
                println!("[xs debug] Timeout waiting for EXECUTE bit to clear");
                break;
            }
            prev_execute = cur_execute;
            // std::thread::sleep(std::time::Duration::from_micros(10));
        }
    }
}
