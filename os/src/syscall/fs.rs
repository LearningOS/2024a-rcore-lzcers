//! File and filesystem-related syscalls
use crate::fs::{get_root_inode, open_file, OpenFlags, Stat};
use crate::mm::{get_phys_addr_from_virt_addr, translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    let Some(fd) = inner.fd_table[_fd].clone() else {
        return -1;
    };
    let fd_stat = fd.stat();

    println!("get fstat...");
    println!("ino: {}, nlink: {}", fd_stat.ino, fd_stat.nlink);
    let phys_addr = get_phys_addr_from_virt_addr(token, _st as usize);
    let _st = phys_addr.get_mut::<Stat>();
    _st.nlink = fd_stat.nlink;
    _st.ino = fd_stat.ino;
    _st.mode = fd_stat.mode;
    return 0;
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    let token = current_user_token();
    // 通过地址读取用户态传递过来的名称数据
    let old_name = translated_str(token, _old_name);
    let new_name = translated_str(token, _new_name);
    let root_node = get_root_inode();
    let Some(inode_id) = root_node.find_inode_id_by_name(&old_name) else {
        println!("Can't find the old_name: {}", &old_name);
        return -1;
    };
    println!(
        "link: {} to: {} inode_id: {}",
        &old_name, &new_name, inode_id
    );
    // 测试用例只涉及根目录的文件链接，所以只考虑root_inode 下创建链接文件即可
    // 将新名称与 inode_id 关联
    root_node.link(&new_name, inode_id);
    0
}

pub fn sys_unlinkat(_name: *const u8) -> isize {
    let token = current_user_token();
    // 通过地址读取用户态传递过来的名称数据
    let name = translated_str(token, _name);
    let root_node = get_root_inode();
    println!("unlink:{}", &name);
    root_node.unlink(&name)
}
