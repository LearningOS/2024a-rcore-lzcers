# 实现

实现了简单的任务状态记录功能，在 TaskControlBlock 上添加 start_time 字段用于记录任务开始时间，增加 syscall_times 字段用于记录系统调用次数。分别在任务第一次运行时，和每次任务运行时检查 start_time 是否为 0，为 0 则 get_time_ms() 初始化任务开始时间，然后在系统调用处增加 Hook 记录系统调用次数。

# 问题

## 1. 正确进入 U 态后，程序的特征还应有：使用 S 态特权指令，访问 S 态寄存器后会报错。 请同学们可以自行测试这些内容（运行 三个 bad 测例 (ch2b*bad*\*.rs) ）， 描述程序出错行为，同时注意注明你使用的 sbi 及其版本。

[rustsbi] RustSBI version 0.3.0-alpha.2, adapting to RISC-V SBI v1.0.0
[kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.
访问非法地址
[kernel] IllegalInstruction in application, kernel killed it.
在用户态调用了 sret
[kernel] IllegalInstruction in application, kernel killed it.
试图读写 sstatus 寄存器

## 2. 深入理解 trap.S 中两个函数 **alltraps** 和 **restore** 的作用，并回答如下问题:

L40：刚进入 **restore** 时，a0 代表了什么值。请指出 **restore** 的两种使用情景。

    a0 寄存器代表着函数的入参，同时也会在返回时作为出参使用。
    刚进入 restore 时，因为 trap_handle 入参 a0 和出参一致，所以 a0 代表着 TrapContext 的地址。
    1.restore 用于处理完中断后回到用户态，2.还有内核初始化后从内核态进入到用户态。

L43-L48：这几行汇编代码特殊处理了哪些寄存器？这些寄存器的的值对于进入用户态有何意义？请分别解释。

    将 CSR 寄存器的状态存入临时寄存器后回写。
    sstatus 寄存器：
    管理和控制特权级别状态。
    比如在 S 态中，SIE 位可以设置为 0 使中断不发生，这样就避免了中断嵌套，默认情况下进入中断状态 SIE 会置为 0，同时保存到 SPIE 中，所以默认不会发生中断嵌套。
    SPIE 记录进入 S 态之前的中断使能情况。
    SPP 记录进入 S 态前的状态。

    SIE (Supervisor Interrupt Enable): 控制 S 模式中断的全局使能。
    SPIE (Supervisor Previous Interrupt Enable): 保存上一次进入 S 模式前的中断使能状态。
    SPP (Supervisor Previous Privilege): 保存上一次进入 S 模式前的特权级别。

    sepc 寄存器：保存中断发生时的指令地址，方便后续返回。
    sscratch 寄存器：用于内核栈和用户栈切换时中转存储栈顶指针 sp，因为所有通用寄存器都可能被使用，需要存储。而 sscratch 作为临时中转存储时只需要一条指令即可完成栈切换。

3. L50-L56：为何跳过了 x2 和 x4？
   x2 是 sp 栈指针， 我们得根据 sp 作为基准来分配栈大小，sp 指针最后在 60 行被恢复了。
   x4 是线程指针，现在用不上。

4. L60：该指令之后，sp 和 sscratch 中的值分别有什么意义？
   L60 后从内核态切换到了用户态，于是 sp 指向用户栈，sscratch 指向内核栈。

5. \_\_restore：中发生状态切换在哪一条指令？为何该指令执行之后会进入用户态？
   发生在 L61 的 sret 指令，该指令在 risc-v 中 用于返回上一层模式，在内核态中 sret 会返回到用户态。
   它会返回到 sepc 寄存器的位置。

6. L13：该指令之后，sp 和 sscratch 中的值分别有什么意义？
   之后 sp 指向内核栈，sscratch 指向用户栈。

7. 从 U 态进入 S 态是哪一条指令发生的？
   从用户态 ecall 调用时硬件就会陷入 S 态的 Trap 处理流程，在 L13 csrrw sp, sscratch, sp，完成用户栈与内核栈的切换。

# 荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

我未做交流

2. 此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

参考了 GPT 给出的 RISC-V 架构寄存器说明。

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
