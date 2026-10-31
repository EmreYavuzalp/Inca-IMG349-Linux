//! Find the mouse's vendor hidraw node and exchange feature reports with it.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::protocol::{self as p, Report, CHUNK, DATA_REPORT, REPORT_LEN};

const VENDOR_ID: u32 = 0x1BCF;
const PRODUCT_ID: u32 = 0x08B8;
const SYSFS: &str = "/sys/class/hidraw";
/// Report ID 41 in the report descriptor marks the vendor interface.
const REPORT_ID_41: [u8; 2] = [0x85, DATA_REPORT];

const fn hidiocsfeature(len: usize) -> libc::c_ulong {
    ((3 << 30) | (len << 16) | ((b'H' as usize) << 8) | 0x06) as libc::c_ulong
}

const fn hidiocgfeature(len: usize) -> libc::c_ulong {
    ((3 << 30) | (len << 16) | ((b'H' as usize) << 8) | 0x07) as libc::c_ulong
}

fn hid_id(node: &Path) -> Option<(u32, u32)> {
    let uevent = fs::read_to_string(node.join("device/uevent")).ok()?;
    let id = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = id.split(':').skip(1);
    let vid = u32::from_str_radix(parts.next()?, 16).ok()?;
    let pid = u32::from_str_radix(parts.next()?, 16).ok()?;
    Some((vid, pid))
}

/// /dev paths of the vendor interface of every connected IMG-349.
pub fn find() -> Vec<PathBuf> {
    let mut nodes: Vec<PathBuf> = match fs::read_dir(SYSFS) {
        Ok(dir) => dir.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return Vec::new(),
    };
    nodes.sort();
    nodes
        .into_iter()
        .filter(|node| {
            if hid_id(node) != Some((VENDOR_ID, PRODUCT_ID)) {
                return false;
            }
            let Ok(desc) = fs::read(node.join("device/report_descriptor")) else { return false };
            desc.windows(2).any(|w| w == REPORT_ID_41)
        })
        .map(|node| Path::new("/dev").join(node.file_name().unwrap()))
        .collect()
}

pub struct Mouse {
    file: File,
}

impl Mouse {
    pub fn open(path: Option<&Path>) -> Result<Self> {
        let path = match path {
            Some(p) => p.to_path_buf(),
            None => match find().into_iter().next() {
                Some(p) => p,
                None => bail!("IMG-349 mouse not found (1bcf:08b8). Is it plugged in?"),
            },
        };
        match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => Ok(Mouse { file }),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => bail!(
                "no permission to open {}: install contrib/70-img349-mouse.rules into /etc/udev/rules.d and re-plug the mouse",
                path.display()
            ),
            Err(e) => Err(e).with_context(|| format!("opening {}", path.display())),
        }
    }

    pub fn send(&self, report: &Report) -> Result<()> {
        let mut buf = *report;
        // SAFETY: HIDIOCSFEATURE reads exactly `buf.len()` bytes from the pointer.
        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), hidiocsfeature(buf.len()) as _, buf.as_mut_ptr()) };
        if rc < 0 {
            bail!("sending report failed: {}", io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn send_all(&self, reports: &[Report]) -> Result<()> {
        for r in reports {
            self.send(r)?;
            std::thread::sleep(Duration::from_millis(p::gap_ms(r)));
        }
        Ok(())
    }

    fn get_chunk(&self) -> Result<[u8; REPORT_LEN]> {
        let mut buf = [0u8; REPORT_LEN];
        buf[0] = DATA_REPORT;
        // SAFETY: HIDIOCGFEATURE writes at most `buf.len()` bytes to the pointer.
        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), hidiocgfeature(buf.len()) as _, buf.as_mut_ptr()) };
        if rc < 0 {
            bail!("reading report failed: {}", io::Error::last_os_error());
        }
        Ok(buf)
    }

    /// Read `len` bytes of the mouse's memory.
    pub fn read(&self, addr: u16, len: usize) -> Result<Vec<u8>> {
        self.send(&p::read_report(addr, len as u16))?;
        let mut out = Vec::with_capacity(len + CHUNK);
        while out.len() < len {
            out.extend_from_slice(&self.get_chunk()?[1..=CHUNK]);
        }
        out.truncate(len);
        Ok(out)
    }
}
