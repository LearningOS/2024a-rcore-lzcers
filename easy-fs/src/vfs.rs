use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use log::debug;
use spin::{Mutex, MutexGuard};
/// Virtual filesystem layer over easy-fs
pub struct Inode {
    inode_id: u32,
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        inode_id: u32,
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            inode_id,
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }
    /// get inode_id
    pub fn get_inode_id(&self) -> u32 {
        self.inode_id
    }

    /// get link num
    pub fn get_link_from_disk_node(&self, inode_id: u32) -> u32 {
        let fs = self.fs.lock();
        let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
        // 修改 disk_inode 的 nlink
        get_block_cache(block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .read(block_offset, |disk_inode: &DiskInode| disk_inode.get_link())
    }

    /// 获取当前 inode 的类型是不是目录
    pub fn is_dir(&self) -> bool {
        let is_dir = |root_inode: &DiskInode| {
            if root_inode.is_dir() {
                true
            } else {
                false
            }
        };
        self.read_disk_inode(is_dir)
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_id() as u32);
            }
        }
        None
    }
    /// Find inode under current inode by name
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    inode_id,
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }
    /// Find inode id
    pub fn find_inode_id_by_name(&self, name: &str) -> Option<u32> {
        self.read_disk_inode(|disk_inode| self.find_inode_id(name, disk_inode))
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }
    /// link
    // 在当前目录下创建一个新的节点，指向INode
    pub fn link(&self, name: &str, inode_id: u32) -> Option<Arc<Inode>> {
        debug!("link!!!");
        let is_dir = |root_inode: &DiskInode| {
            if root_inode.is_dir() {
                true
            } else {
                false
            }
        };
        // 判断当前 inode 是目录节点，这样才能在之下创建文件 inode
        if !self.read_disk_inode(is_dir) {
            return None;
        }

        let mut fs = self.fs.lock();
        // 简单在当前根目录的 DirEntry 中加一项指向被链接的索引 inode_id 不就行了吗？
        // 都不用单独创建 disk_inode
        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });
        let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
        drop(fs);

        self.update_link_num(inode_id, 0);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            inode_id,
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
    }

    /// 更新 nlink, flag 0 add 1 sub
    pub fn update_link_num(&self, inode_id: u32, flag: u8) -> u32 {
        let fs = self.fs.lock();
        let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
        // 修改 disk_inode 的 nlink
        get_block_cache(block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(block_offset, |disk_inode: &mut DiskInode| {
                if flag == 0 {
                    disk_inode.add_link();
                    debug!("add link");
                } else if flag == 1 {
                    disk_inode.sub_link();
                    debug!("sub link");
                }
                disk_inode.get_link()
            })
    }

    /// unlink
    pub fn unlink(&self, name: &str) -> isize {
        debug!("unlink!!!");
        let is_dir = |root_inode: &DiskInode| {
            if root_inode.is_dir() {
                true
            } else {
                false
            }
        };
        if !self.read_disk_inode(is_dir) {
            return -1;
        }

        // 修改 disk_inode 的 nlink
        self.modify_disk_inode(|root_inode| {
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            // 定位到目标 DirEntry 的偏移
            // 创建一个空的目录项
            let mut dirent = DirEntry::empty();
            // 遍历所有文件项,找到对应 dirEntry 的偏移
            for de_i in 0..file_count {
                root_inode.read_at(DIRENT_SZ * de_i, dirent.as_bytes_mut(), &self.block_device);
                if dirent.name() == name {
                    let inode_id = dirent.inode_id();
                    // 更新 nlink
                    self.update_link_num(inode_id, 1);
                    // 直接用空项覆盖
                    root_inode.write_at(
                        DIRENT_SZ * de_i,
                        DirEntry::empty().as_bytes(),
                        &self.block_device,
                    );
                    return 0;
                }
            }
            return -1;
        })
    }

    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });

        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            new_inode_id,
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }
}
