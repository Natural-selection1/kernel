# TinyArc 的 Send / Sync 约束审查

记录日期：2026-11-06。
源码基线：main `7e124fa98f6acd4561736d4325a2edcb0f5b3de2`。

本文记录一次源码层面的影响分析：如果将 TinyArc 的 Send / Sync 收紧到标准 Arc 的 payload 约束，会暴露哪些调用者与类型契约问题。它不是 Miri 运行记录，也不是完整的编译失败清单。本次没有修改 trait 实现，没有执行收紧后的全配置构建，没有证明下列类型应该新增或删除任何 unsafe 实现。

## 假设的改动

[TinyArc](src/tinyarc/mod.rs) 当前无条件实现 Send 和 Sync。拟调查的最小改动仅为：

```rust
unsafe impl<T: Send + Sync> Send for TinyArc<T> {}
unsafe impl<T: Send + Sync> Sync for TinyArc<T> {}
```

这两个约束只控制所有者能否跨线程转移和共享，不限制 `TinyArc<T>` 的构造、克隆以及单线程使用。错误会在闭包要求 Send、共享静态变量要求 Sync，或外围类型需要这些 trait 时出现。

两者都需要 T: Send + Sync：不同所有者可以同时取得 &T；任一持有者也可能释放最后一个引用，在该线程执行 T 的析构。仅仅保证堆中值没有移动，不足以保证跨线程所有权和析构安全。

## 源码能够直接推导的影响

以下类型均明确声明 `!Send`，并手动实现 Sync。收紧之后，它们的 TinyArc 将既不是 Send，也不是 Sync。

| 类型 | 声明位置 | 已发现的影响边界 |
| --- | --- | --- |
| MemoryPool | [mempool.rs](../adapter/common/src/mempool.rs) | 三个多线程测试把克隆的所有者捕获进 `thread::spawn` 闭包 |
| MessageQueue | [mqueue.rs](../kernel/src/sync/mqueue.rs) | 多线程收发测试把队列所有者传给工作线程；[内核测试](../kernel/src/lib.rs)还有 `static MQUEUE: Lazy<Arc<MessageQueue>>` |
| Mutex | [mutex.rs](../kernel/src/sync/mutex.rs) | 多线程测试传递互斥量所有者；限制也会沿 `Vec<Arc<Mutex>>` 等外层集合传播 |
| Semaphore | [semaphore.rs](../kernel/src/sync/semaphore.rs) | 工作线程持有克隆的信号量所有者 |
| Thread | [thread/mod.rs](../kernel/src/thread/mod.rs) | `ThreadNode = Arc<Thread>` 将失去 Send / Sync，影响要求这些 trait 的外围对象与调用边界 |
| EventFlags | [event_flags.rs](../kernel/src/sync/event_flags.rs) | 所有者将失去 Send / Sync；具体受影响构建目标尚未枚举 |

[thread::spawn](../kernel/src/thread/builder.rs) 明确要求 `F: FnOnce() + Send + 'static`。例如 MemoryPool 的依赖链为：

```text
MemoryPool: !Send
    → TinyArc<MemoryPool>: !Send
    → 捕获该所有者的闭包: !Send
    → 不满足 thread::spawn 的约束
```

这是现有类型声明与所有权用法之间的冲突，不自动意味着对象的多线程功能设计错误，也不构成删除 `!Send` 的依据。

## 如何解释 !Send + Sync

`T: Sync` 允许跨线程共享 &T；转移一个 Arc<T> 则同时交出了一份所有权和最终析构的可能性。这两种权限不同。

需要对每个对象分别回答：

- `!Send` 是否原本用来防止移动、自引用失效或侵入式节点失效？这些要求应由地址稳定、初始化和 Pin 契约表达，Send 本身不禁止同线程移动。
- 对象是否真的只能在某个线程、CPU 或执行上下文析构？如果是，普通跨线程共享所有者的最终析构机制可能不符合需求。
- 内部裸指针、等待队列、外部存储与锁保护是否已经足以支持跨线程转移和访问？如果没有，应先建立论证。

MemoryPool 为多线程使用而设计，并不能单独证明 MemoryPool: Send；反过来，当前写了 `!Send` 也不能证明这是正确的设计意图。不能为了恢复编译批量删除负实现或补充 unsafe impl。

## 泛型接口与可能掩盖错误的包装

[BusWrapper](../kernel/src/devices/bus/mod.rs) 的表示为：

```rust
pub struct BusWrapper<B: BusInterface>(TinyArc<SpinLock<B>>);
```

BusInterface 当前没有 Send / Sync 约束。按现有 [SpinLock](../kernel/src/sync/spinlock.rs) 实现，Send 依赖 B: Send，Sync 依赖 B: Sync。因此收紧 TinyArc 后，BusWrapper<B> 的这两个 trait 都需要 B: Send + Sync；需要跨线程使用该包装的泛型接口可能缺少相应约束。仅定义或在本线程构造 BusWrapper 不会因此失败。

这里是在描述当前约束的传播，不是在认可 SpinLock 本身的 unsafe 实现已经充分论证。

同一文件的 Bus<B> 已经手动无条件实现 Send / Sync。这些 unsafe 实现不会自动因为字段失去相应 trait 而报错；编译器会相信它们。因此，收紧 TinyArc 后仍然通过编译的外围包装也需要审查。

类似地，[TinyArcInner](src/tinyarc/mod.rs) 自身仍有无条件 Send / Sync 实现。只改外层 TinyArc 不会审查或关闭控制块层面的保证。静态控制块、裸指针传递、`static mut` 和其他手写 unsafe 包装，都不能靠此次两行约束变更完成验证。

## 可以筛掉什么，不能筛掉什么

普通原子计数器及确实满足 Send + Sync 的共享值通常不受影响。`TinyArc<Cell<_>>` 跨线程共享则会被拒绝，因为 Cell 不满足 Sync。

以下问题不会因收紧 TinyArc 自动消失：

| 问题 | 原因 |
| --- | --- |
| NoLock 的冲突访问与数据竞争 | NoLock 已手动实现相应 trait，例如 NoLock<usize> 仍满足外层要求；单线程别名问题也与这些 trait 无关 |
| TinyArcCas 的内部可变性与回收契约 | payload 满足 Send + Sync，不能证明将普通字段强转为原子存储的实现正确 |
| 链表、侵入式堆、树的别名冲突 | 固定地址、有效生命周期和合法引用访问是独立要求 |
| get_mut_unchecked 的不合法调用 | unsafe 调用方义务不会由所有者的线程 trait 自动检查 |
| Storage 的初始化与字节访问问题 | 线程约束不表达初始化状态 |
| 引用计数溢出及其他释放问题 | payload 约束不验证计数和回收算法 |

## 后续独立实验建议

本节是尚未实施的建议，不是本提交完成的工作。

1. 在独立分支只修改 TinyArc 外层的两处实现，暂不修补调用者。
2. 构建宿主 infra 测试、相关内核配置及 adapter，区分生产目标与测试目标；保留配置、编译器版本和原始错误日志。
3. 将诊断按根因聚类：显式 !Send / !Sync、字段导致自动推导失败、泛型缺少约束。多个调用点可能来自同一个类型声明。
4. 单列被外围 unsafe impl 遮住的类型，并审查 TinyArcInner、静态控制块以及裸指针接口。
5. 配套正反编译测试，确认普通共享值仍可用，Cell、非 Send payload 等受到预期限制。
6. 对每个冲突对象先确定地址稳定、访问同步和析构上下文，再决定修改对象设计还是所有权使用方式。

该实验得到的是当前类型系统暴露出的依赖关系，不是整个 infra 的线程安全证明，也不能替代运行期并发与别名检查。
