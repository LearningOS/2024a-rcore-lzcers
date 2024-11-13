# 实现的功能

主要实现了 sys_spawn 和 stride 调度算法， spawn 只需要创建 CTB 即可，配合 exec 函数替换 CTB 的地址空间内容和 Ctx 然后插入任务队列即可。
stride 的调度算法实现也很简单。

# 问答

stride 算法原理非常简单，但是有一个比较大的问题。例如两个 pass = 10 的进程，使用 8bit 无符号整形储存 stride， p1.stride = 255, p2.stride = 250，在 p2 执行一个时间片后，理论上下一次应该 p1 执行。

1. 实际情况是轮到 p1 执行吗？为什么？
   不一定， p2 执行后加上 pass 它会整数溢出，如果是环绕溢出的话，那么 p2 会从 4 开始，那就轮不到 p1 执行了。

2. 为什么？尝试简单说明（不要求严格证明）。
   假设都是从 0 开始，优先级都是 2, 某时刻 p1 < p2， p2 - p1 = BigStride / 2, 即 stride 步长相差一个 pass, pass = BigStride / 2, 那么 strideMax - strideMin = BigStride / 2 ,优先级更大的情况只会比这个小。

已知以上结论，考虑溢出的情况下，可以为 Stride 设计特别的比较器，让 BinaryHeap<Stride> 的 pop 方法能返回真正最小的 Stride。补全下列代码中的 partial_cmp 函数，假设两个 Stride 永远不会相等。

```Rust
use core::cmp::Ordering;

struct Stride(u64);

impl PartialOrd for Stride {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // 意味着反转
        if (self.0 - other.0).abs() > BigStride / 2 {
            if self.0 < self.1 {
                Some(Greater)
            } else {
                Some(Ordering::Less)

            }
        }
        else {
            // 正常返回即可
            if self.0 < self.1 {
                Some(Less)
            } else {
                Some(Greater)
            }
        }
    }
}

impl PartialEq for Stride {
    fn eq(&self, other: &Self) -> bool {
        false
    }
}
```

# 荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

    未交流

2. 此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

    参考了其它同学的实现，终于发现自己粗心大意写错页表有效性检测的代码了，漏了 ! 符号 😓，其它完全按照自己思路实现。

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
