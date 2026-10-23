//! The five metadata fields the walker needs, from `std::fs::Metadata`,
//! on each platform. Unix has them all; Windows has no allocated size or
//! link count in the metadata a directory listing returns, so size is
//! rounded up to 4 KiB clusters and every file counts as one link. The
//! MFT reader, not this, is the real Windows path.

#[cfg(unix)]
mod imp {
    use std::fs::Metadata;
    use std::os::unix::fs::MetadataExt;

    pub fn dev(md: &Metadata) -> u64 {
        md.dev()
    }
    pub fn mtime(md: &Metadata) -> i64 {
        md.mtime()
    }
    pub fn mode(md: &Metadata) -> u32 {
        md.mode()
    }
    pub fn allocated(md: &Metadata) -> u64 {
        md.blocks() * 512
    }
    pub fn nlink(md: &Metadata) -> u64 {
        md.nlink()
    }
}

#[cfg(windows)]
mod imp {
    use std::fs::Metadata;
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_READONLY: u32 = 0x1;

    pub fn dev(_md: &Metadata) -> u64 {
        0
    }
    pub fn mtime(md: &Metadata) -> i64 {
        (md.last_write_time() / 10_000_000) as i64 - 11_644_473_600
    }
    pub fn mode(md: &Metadata) -> u32 {
        if md.is_dir() {
            0o040755
        } else if md.file_type().is_symlink() {
            0o120777
        } else if md.file_attributes() & FILE_ATTRIBUTE_READONLY != 0 {
            0o100444
        } else {
            0o100644
        }
    }
    // ponytail: assumes 4 KiB clusters; the MFT reader has the real figure.
    pub fn allocated(md: &Metadata) -> u64 {
        md.file_size().div_ceil(4096) * 4096
    }
    pub fn nlink(_md: &Metadata) -> u64 {
        1
    }
}

pub use imp::{allocated, dev, mode, mtime, nlink};
