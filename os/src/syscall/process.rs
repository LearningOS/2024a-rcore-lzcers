//! Process management syscalls
use alloc::sync::Arc;

use crate::{
    config::MAX_SYSCALL_NUM,
    loader::get_app_data_by_name,
    mm::{
        get_phys_addr_from_virt_addr, translated_refmut, translated_str, MapPermission, PageTable,
        VPNRange, VirtAddr,
    },
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        get_current_task_info, insert_frame_to_current_task, suspend_current_and_run_next,
        TaskStatus,
    },
    timer::{get_time_ms, get_time_us},
};

#[repr(C)]
#[derive(Debug)]
/// timeVal
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
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

/// get pid
pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

/// sys fork
pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

/// sys exec
pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!(
        "kernel::pid[{}] sys_waitpid [{}]",
        current_task().unwrap().pid.0,
        pid
    );
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    println!("sys_spawn");
    // spawn 不需要复制进程空间的资源
    // 我们不需要拷贝地址空间，只需要创建新的 PidHandle 等信息
    // 从当前进程中 spawn 一个块
    // 通过 _path 取到应用程序的数据
    let token = current_user_token();
    let path = translated_str(token, _path);

    if let Some(data) = get_app_data_by_name(path.as_str()) {
        // 成功获取到 app 数据
        println!("get app data suscess: {:?}", _path);
        // 创建一个 task ，此时地址空间，trap 均没有设置
        let new_task = current_task().unwrap().spawn();
        new_task.exec(data);
        let pid = new_task.getpid() as isize;
        let t_cx = new_task.inner_exclusive_access().get_trap_cx();
        t_cx.x[10] = 0;
        add_task(new_task);
        pid
    } else {
        println!("get app data failed.");
        -1
    }
}

/// set
// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    let task = current_task().unwrap();
    println!("set priority: {}", _prio);
    task.set_priority(_prio)
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

    let info = get_current_task_info();
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

    insert_frame_to_current_task(start_virt_addr, end_virt_addr, flag);
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
