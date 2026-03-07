/// Memory-mapped file management for .brain files.
///
/// SAFETY: This module contains all unsafe mmap operations.
/// All other modules access data through safe abstractions provided here.

use crate::storage::error::{Result, StorageError};
use crate::storage::header::{Header, HEADER_SIZE};
use std::fs::{File, OpenOptions};
use std::path::Path;

pub struct MmapFile {
    file: File,
    ptr: *mut u8,
    len: u64,
}

// SAFETY: MmapFile manages its own memory and file handle.
// Access is controlled through single-writer/multiple-reader locking
// at the BrainFile level.
unsafe impl Send for MmapFile {}
unsafe impl Sync for MmapFile {}

impl MmapFile {
    /// Create a new file and mmap it with the given size.
    pub fn create(path: &Path, size: u64) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;

        // Set file size
        file.set_len(size)?;

        let ptr = Self::map_file(&file, size)?;

        Ok(MmapFile {
            file,
            ptr,
            len: size,
        })
    }

    /// Open an existing file and mmap it.
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;

        let metadata = file.metadata()?;
        let size = metadata.len();

        if size < HEADER_SIZE {
            return Err(StorageError::InvalidFile {
                reason: format!("file too small: {} bytes (minimum {})", size, HEADER_SIZE),
            });
        }

        let ptr = Self::map_file(&file, size)?;

        Ok(MmapFile {
            file,
            ptr,
            len: size,
        })
    }

    /// Get a pointer to a specific offset in the mapped region.
    ///
    /// SAFETY: Caller must ensure offset + size_of::<T>() <= self.len
    pub unsafe fn ptr_at(&self, offset: u64) -> *const u8 {
        // SAFETY: offset is bounds-checked by callers
        unsafe { self.ptr.add(offset as usize) }
    }

    /// Get a mutable pointer to a specific offset.
    ///
    /// SAFETY: Caller must ensure offset + size_of::<T>() <= self.len
    /// and that no other references exist to this region.
    pub unsafe fn ptr_at_mut(&self, offset: u64) -> *mut u8 {
        // SAFETY: offset is bounds-checked by callers
        unsafe { self.ptr.add(offset as usize) }
    }

    /// Read the header from the mapped file.
    pub fn read_header(&self) -> &Header {
        // SAFETY: Header is at offset 0, file is at least HEADER_SIZE bytes,
        // and Header is repr(C) with a known layout.
        unsafe { &*(self.ptr as *const Header) }
    }

    /// Get a mutable reference to the header.
    ///
    /// SAFETY: Caller must ensure exclusive write access.
    pub unsafe fn header_mut(&self) -> &mut Header {
        // SAFETY: Caller guarantees exclusive access
        unsafe { &mut *(self.ptr as *mut Header) }
    }

    /// Flush changes to disk.
    pub fn flush(&self) -> Result<()> {
        #[cfg(windows)]
        {
            // SAFETY: ptr and len are valid from the mapping
            let result =
                unsafe { windows_flush_view(self.ptr, self.len as usize) };
            if !result {
                return Err(StorageError::Io(std::io::Error::last_os_error()));
            }
        }
        #[cfg(unix)]
        {
            // SAFETY: ptr and len are valid from the mapping
            let result = unsafe {
                libc::msync(self.ptr as *mut libc::c_void, self.len as usize, libc::MS_SYNC)
            };
            if result != 0 {
                return Err(StorageError::Io(std::io::Error::last_os_error()));
            }
        }
        Ok(())
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    /// Resize the mapped file to a new (larger) size.
    /// Unmaps the current view, extends the file, and remaps.
    ///
    /// SAFETY: All pointers/references into the old mapping become invalid.
    /// Caller must ensure no references to mmap'd data are held across this call.
    pub fn remap(&mut self, new_size: u64) -> Result<()> {
        if new_size <= self.len {
            return Ok(());
        }

        // Step 1: Flush pending changes
        self.flush()?;

        // Step 2: Unmap current view
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Memory::UnmapViewOfFile(
                windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.ptr as *mut std::ffi::c_void,
                },
            );
        }
        #[cfg(unix)]
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.len as usize);
        }

        // Step 3: Extend the file
        self.file.set_len(new_size)?;

        // Step 4: Remap with new size
        self.ptr = Self::map_file(&self.file, new_size)?;
        self.len = new_size;

        Ok(())
    }

    #[cfg(windows)]
    fn map_file(file: &File, size: u64) -> Result<*mut u8> {
        use std::os::windows::io::AsRawHandle;

        // SAFETY: Windows API calls for creating a file mapping.
        // The file handle is valid and size is set correctly.
        unsafe {
            let handle = file.as_raw_handle() as *mut std::ffi::c_void;

            let mapping = windows_sys::Win32::System::Memory::CreateFileMappingW(
                handle,
                std::ptr::null(),
                windows_sys::Win32::System::Memory::PAGE_READWRITE,
                (size >> 32) as u32,
                size as u32,
                std::ptr::null(),
            );

            if mapping.is_null() {
                return Err(StorageError::MmapFailed {
                    reason: format!(
                        "CreateFileMappingW failed: {}",
                        std::io::Error::last_os_error()
                    ),
                });
            }

            let ptr = windows_sys::Win32::System::Memory::MapViewOfFile(
                mapping,
                windows_sys::Win32::System::Memory::FILE_MAP_ALL_ACCESS,
                0,
                0,
                size as usize,
            );

            // Close the mapping handle -- the view keeps the mapping alive
            windows_sys::Win32::Foundation::CloseHandle(mapping);

            if ptr.Value.is_null() {
                return Err(StorageError::MmapFailed {
                    reason: format!(
                        "MapViewOfFile failed: {}",
                        std::io::Error::last_os_error()
                    ),
                });
            }

            Ok(ptr.Value as *mut u8)
        }
    }

    #[cfg(unix)]
    fn map_file(file: &File, size: u64) -> Result<*mut u8> {
        use std::os::unix::io::AsRawFd;

        // SAFETY: Unix mmap call with valid fd and size.
        unsafe {
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                size as usize,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            );

            if ptr == libc::MAP_FAILED {
                return Err(StorageError::MmapFailed {
                    reason: format!("mmap failed: {}", std::io::Error::last_os_error()),
                });
            }

            Ok(ptr as *mut u8)
        }
    }
}

impl Drop for MmapFile {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Memory::UnmapViewOfFile(
                windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.ptr as *mut std::ffi::c_void,
                },
            );
        }
        #[cfg(unix)]
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.len as usize);
        }
    }
}

#[cfg(windows)]
unsafe fn windows_flush_view(ptr: *mut u8, len: usize) -> bool {
    // SAFETY: ptr and len are valid from the mapping
    unsafe {
        windows_sys::Win32::System::Memory::FlushViewOfFile(ptr as *const std::ffi::c_void, len)
            != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn create_and_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");

        let size = HEADER_SIZE + 256 * 10; // header + 10 nodes

        // Create
        {
            let mmap = MmapFile::create(&path, size).unwrap();
            assert_eq!(mmap.len(), size);

            // Write header
            unsafe {
                let header = mmap.header_mut();
                *header = Header::new(10, 10);
            }
            mmap.flush().unwrap();
        }

        // Reopen
        {
            let mmap = MmapFile::open(&path).unwrap();
            let header = mmap.read_header();
            assert!(header.validate().is_ok());
            assert_eq!(header.node_region_capacity, 10);
        }
    }
}
