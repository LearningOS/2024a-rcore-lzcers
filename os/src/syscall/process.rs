//! Process management syscalls

use crate::{
    config::MAX_SYSCALL_NUM,
    mm::{get_phys_addr_from_virt_addr, MapPermission, PageTable, VPNRange, VirtAddr},
    task::{
        change_program_brk, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus, TASK_MANAGER,
    },
    timer::{get_time_ms, get_time_us},
};

/// timeval
#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    /// sec
    pub sec: usize,
    /// usec
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    // 问题在于如何从内核地址空间访问应用地址空间的数据
    // 此时 ts 作为一个裸指针是一个虚拟地址
    // 需要找到当前陷入应用的页表，将虚拟地址转换为对物理地址
    // 如何找到陷入应用的页表？
    // 查 satp 寄存器可以拿到当前应用的根 PPN，PPN 应该可以定位页表
    // 但是所有的页表存在哪呢？
    // TaskManager 可以拿到所有的任务以及页表
    // 如何通过页表找到物理地址？
    // 通过页表索引转换取物理地址

    trace!("kernel: sys_get_time");
    let phys_addr = get_phys_addr_from_virt_addr(current_user_token(), ts as usize);
    let us = get_time_us();
    let ts_addr = phys_addr.get_mut::<TimeVal>();
    *ts_addr = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    let phys_addr = get_phys_addr_from_virt_addr(current_user_token(), _ti as usize);
    let ts_addr = phys_addr.get_mut::<TaskInfo>();

    let info = TASK_MANAGER.get_current_task_info();
    ts_addr.time = get_time_ms() - info.0;
    ts_addr.syscall_times = info.1;
    ts_addr.status = info.2;
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    // 申请一块 len 长度的内存，将其页属性设置为 port
    // 将内存写入 start 开始的页表
    // len 长度如何转换为多少页？
    // len / page_size 转换为页, 向上取整
    // 使用 VPNRange + map 进行连续内存分配

    // 检查 Port
    if (_port & 0x7 == 0) | (_port & !0x7 != 0) {
        println!("invalid port");
        return -1;
    }

    let end = _start + _len;
    let start_virt_addr = VirtAddr::from(_start);
    let end_virt_addr = VirtAddr::from(end);

    // 检查地址
    if !start_virt_addr.aligned() {
        println!("virt addr not page aligned");
        return -1;
    }

    let start_page = start_virt_addr.floor();
    let end_page = end_virt_addr.ceil();
    let token = current_user_token();
    let page_table = PageTable::from_token(token);
    let flag_bits = (_port << 1 | (1 << 4)) as u8;
    let flag = MapPermission::from_bits(flag_bits).unwrap();
    println!("start page: {:?} end page: {:?}", start_page, end_page);
    println!("bits flag: {:?}", flag);

    for n in start_page.0..end_page.0 {
        let vpn = n.into();
        if let Some(pte) = page_table.find_pte(vpn) {
            if pte.is_valid() {
                println!("vpn is mappend: {:?} ", vpn);
                return -1;
            }
        }
    }

    TASK_MANAGER.insert_frame_to_current_task(start_virt_addr, end_virt_addr, flag);
    0
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    let token = current_user_token();
    let mut page_table = PageTable::from_token(token);
    let end = _start + _len;
    let start_virt_addr = VirtAddr::from(_start);
    let end_virt_addr = VirtAddr::from(end);
    let vpn_range = VPNRange::new(start_virt_addr.floor(), end_virt_addr.ceil());
    for vpn in vpn_range {
        println!("unmapping {:?}", vpn);
        let pte = page_table.find_pte(vpn).unwrap();
        if !pte.is_valid() {
            println!("invalid vpn: {:?}", vpn);
            return -1;
        } else {
            page_table.unmap(vpn);
        }
    }
    0
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
