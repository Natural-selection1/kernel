# main infra Miri 实测汇总

基线：`7e124fa98f6acd4561736d4325a2edcb0f5b3de2`；日期：2026-11-06。
工具链：nightly-2026-05-24；默认 Stacked Borrows 与 Tree Borrows；每例上限 60 秒。
113 项原有测试 + 16 项新增契约测试 = 每种模型 129 项。宿主原有 113 项全部通过。
测试失败数量不等于独立缺陷数量；超时属于未决项，泄漏单列。

| 模型 | 通过 | UB 诊断 | 泄漏 | 超时 | 其他失败 |
| --- | ---: | ---: | ---: | ---: | ---: |
| stacked | 71 | 56 | 2 | 0 | 0 |
| tree | 77 | 49 | 2 | 1 | 0 |

唯一超时用例 `list::singly_linked_list::tests::test_push` 提高到 180 秒上限后通过，耗时 97.916 秒。原始 60 秒结果保留，不把超时当作缺陷。
[延时复测日志](tree-slow-recheck/results.md)。计入复测后，Tree Borrows 为 **78 通过、49 UB 诊断、2 泄漏、0 未决超时**。

## 按模块分布

每格为「通过 / UB / 泄漏 / 超时」。

| 模块 | 测试数 | Stacked | Tree |
| --- | ---: | --- | --- |
| iheap | 10 | 2 / 8 / 0 / 0 | 2 / 8 / 0 / 0 |
| intrusive | 1 | 1 / 0 / 0 / 0 | 1 / 0 / 0 / 0 |
| lifetime | 1 | 1 / 0 / 0 / 0 | 1 / 0 / 0 / 0 |
| list::singly_linked_list | 11 | 11 / 0 / 0 / 0 | 10 / 0 / 0 / 1 |
| list::typed_atomic_ilist | 5 | 0 / 5 / 0 / 0 | 1 / 4 / 0 / 0 |
| list::typed_ilist | 14 | 2 / 10 / 2 / 0 | 3 / 9 / 2 / 0 |
| nolock | 2 | 2 / 0 / 0 / 0 | 2 / 0 / 0 / 0 |
| rbtree | 6 | 1 / 5 / 0 / 0 | 6 / 0 / 0 / 0 |
| ringbuffer | 10 | 10 / 0 / 0 / 0 | 10 / 0 / 0 / 0 |
| spinarc | 9 | 9 / 0 / 0 / 0 | 9 / 0 / 0 / 0 |
| string | 7 | 7 / 0 / 0 / 0 | 7 / 0 / 0 / 0 |
| tinyarc | 17 | 5 / 12 / 0 / 0 | 5 / 12 / 0 / 0 |
| tinyarc::sorted_list | 5 | 2 / 3 / 0 / 0 | 2 / 3 / 0 / 0 |
| tinyrwlock | 15 | 15 / 0 / 0 / 0 | 15 / 0 / 0 / 0 |
| 新增契约测试 | 16 | 3 / 13 / 0 / 0 | 3 / 13 / 0 / 0 |

## 按首个诊断归类

仅聚类诊断，不把聚类数量当成根因数量。

- stacked: 借用/别名权限 45, 数据竞争 4, 读取未初始化字节 2, 零大小分配 2, 无效引用/NonNull 3

- tree: 借用/别名权限 38, 数据竞争 4, 读取未初始化字节 2, 零大小分配 2, 无效引用/NonNull 3

## 模型结果不同的用例

| 测试 | Stacked | Tree |
| --- | --- | --- |
| list::singly_linked_list::tests::test_push | passed | timeout |
| list::typed_atomic_ilist::tests::test_basic | undefined-behavior | passed |
| list::typed_ilist::tests::test_basic | undefined-behavior | passed |
| rbtree::tests::test_delete_complex | undefined-behavior | passed |
| rbtree::tests::test_delete_root_scenarios | undefined-behavior | passed |
| rbtree::tests::test_insert_and_search | undefined-behavior | passed |
| rbtree::tests::test_iterator | undefined-behavior | passed |
| rbtree::tests::test_sequential_insert_stress | undefined-behavior | passed |

## 全部非通过用例及原始日志

每段日志保留原始调用命令；执行目录为 `out/miri-audit`。归档移除了终端颜色转义序列与行尾空白，行号链接指向对应日志段。命令中的绝对路径需按本机环境调整。

### stacked

| 测试 | 分类 | 首个源码位置 | 诊断/日志 |
| --- | --- | --- | --- |
| iheap::tests::concurrent_stress | undefined-behavior | kernel/infra/src/iheap.rs:246:30 | [Undefined Behavior: trying to retag from <240019> for Unique permission at alloc64492[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1) |
| iheap::tests::fuzz | undefined-behavior | kernel/infra/src/iheap.rs:246:30 | [Undefined Behavior: trying to retag from <174600> for Unique permission at alloc53387[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L48) |
| iheap::tests::fuzz1 | undefined-behavior | kernel/infra/src/iheap.rs:246:30 | [Undefined Behavior: trying to retag from <173868> for Unique permission at alloc53001[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L86) |
| iheap::tests::fuzz2 | undefined-behavior | kernel/infra/src/iheap.rs:246:30 | [Undefined Behavior: trying to retag from <229336> for Unique permission at alloc64065[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L124) |
| iheap::tests::test_basic_insertion | undefined-behavior | kernel/infra/src/iheap.rs:246:30 | [Undefined Behavior: trying to retag from <173618> for SharedReadOnly permission at alloc53049[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L171) |
| iheap::tests::test_removal | undefined-behavior | kernel/infra/src/iheap.rs:246:30 | [Undefined Behavior: trying to retag from <173875> for Unique permission at alloc53075[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L218) |
| iheap::tests::test_remove_sibling | undefined-behavior | kernel/infra/src/iheap.rs:246:30 | [Undefined Behavior: trying to retag from <173308> for Unique permission at alloc52924[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L256) |
| iheap::tests::test_stack_removal | undefined-behavior | kernel/infra/src/iheap.rs:246:30 | [Undefined Behavior: trying to retag from <173283> for Unique permission at alloc52942[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L300) |
| list::typed_atomic_ilist::tests::test_basic | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:64:18 | [Undefined Behavior: trying to retag from <172391> for SharedReadOnly permission at alloc52724[0x10], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L461) |
| list::typed_atomic_ilist::tests::test_concurrent_ops | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:124:37 | [Undefined Behavior: trying to retag from <178950> for Unique permission at alloc53069[0x8], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L492) |
| list::typed_atomic_ilist::tests::test_concurrent_ops1 | undefined-behavior | kernel/infra/src/tinyarc/mod.rs:202:28 | [Undefined Behavior: Data race detected between (1) retag read on thread `list::typed_ato` and (2) retag write of type `tinyarc::TinyArcInner<list::typed_atomic_ilist::tests::Foo>` on thread `unnamed-2` at alloc53072+0x8](stacked/cases.txt#L543) |
| list::typed_atomic_ilist::tests::test_concurrent_ops2 | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:391:35 | [Undefined Behavior: Data race detected between (1) retag write on thread `unnamed-2` and (2) retag read of type `tinyarc::TinyArcInner<list::typed_atomic_ilist::tests::Foo>` on thread `list::typed_ato` at alloc53147+0x8](stacked/cases.txt#L587) |
| list::typed_atomic_ilist::tests::test_insert_and_detach | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:124:37 | [Undefined Behavior: trying to retag from <173286> for Unique permission at alloc52909[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L623) |
| list::typed_ilist::tests::test_basic | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:133:18 | [Undefined Behavior: trying to retag from <173108> for SharedReadOnly permission at alloc52926[0x10], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L675) |
| list::typed_ilist::tests::test_insert_and_detach | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:153:41 | [Undefined Behavior: trying to retag from <173337> for Unique permission at alloc52949[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L706) |
| list::typed_ilist::tests::test_insert_before | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:179:41 | [Undefined Behavior: trying to retag from <173214> for Unique permission at alloc52926[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L744) |
| list::typed_ilist::tests::test_list_basic | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:153:41 | [Undefined Behavior: trying to retag from <173598> for Unique permission at alloc53022[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L782) |
| list::typed_ilist::tests::test_list_basic_hole | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:153:41 | [Undefined Behavior: trying to retag from <173500> for Unique permission at alloc53023[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L826) |
| list::typed_ilist::tests::test_list_push_and_pop_several | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:153:41 | [Undefined Behavior: trying to retag from <173405> for Unique permission at alloc52964[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L870) |
| list::typed_ilist::tests::test_list_push_by_and_pop_several | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:151:42 | [Undefined Behavior: trying to retag from <173685> for Unique permission at alloc53061[0x10], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L914) |
| list::typed_ilist::tests::test_list_reorder_chosen_value | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:151:42 | [Undefined Behavior: trying to retag from <173389> for Unique permission at alloc52964[0x10], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L963) |
| list::typed_ilist::tests::test_listiterator | leak | kernel/infra/src/list/typed_ilist.rs:528:21 | [memory leaked: alloc52775 (Rust heap, size: 16, align: 8), allocated here:](stacked/cases.txt#L1012) |
| list::typed_ilist::tests::test_listreverseiterator | leak | kernel/infra/src/list/typed_ilist.rs:576:25 | [memory leaked: alloc53078 (Rust heap, size: 16, align: 8), allocated here:](stacked/cases.txt#L1077) |
| list::typed_ilist::tests::test_safer_insert_and_detach | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:153:41 | [Undefined Behavior: trying to retag from <172395> for Unique permission at alloc52574[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1128) |
| list::typed_ilist::tests::test_safer_insert_and_detach_several | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:153:41 | [Undefined Behavior: trying to retag from <173002> for Unique permission at alloc52868[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1170) |
| miri_contract_tests::intrusive_nolock_projects_mutable_owner | undefined-behavior | kernel/infra/src/nolock.rs:143:18 | [Undefined Behavior: trying to retag from <173032> for Unique permission at alloc52902[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1212) |
| miri_contract_tests::intrusive_rwlock_projects_mutable_owner | undefined-behavior | kernel/infra/src/tinyrwlock.rs:849:18 | [Undefined Behavior: trying to retag from <173403> for Unique permission at alloc52904[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1245) |
| miri_contract_tests::nolock_overlapping_mutable_references | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:102:5 | [Undefined Behavior: attempting a write access using <173629> at alloc53040[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1278) |
| miri_contract_tests::nolock_reader_overlaps_writer | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:114:26 | [Undefined Behavior: attempting a read access using <173512> at alloc53017[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1312) |
| miri_contract_tests::nolock_shared_across_threads | undefined-behavior | kernel/infra/src/nolock.rs:52:18 | [Undefined Behavior: Data race detected between (1) non-atomic write on thread `unnamed-2` and (2) retag write of type `i32` on thread `miri_contract_t` at alloc52943+0x10](stacked/cases.txt#L1346) |
| miri_contract_tests::ringbuffer_owned_writer_exposes_uninitialized_bytes | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:63:26 | [Undefined Behavior: reading memory at alloc52651[0x0..0x1], but memory is uninitialized at [0x0..0x1], and this operation requires initialized memory](stacked/cases.txt#L1380) |
| miri_contract_tests::ringbuffer_zero_capacity | undefined-behavior | kernel/infra/src/storage.rs:33:29 | [Undefined Behavior: creating allocation with size 0](stacked/cases.txt#L1409) |
| miri_contract_tests::storage_empty_mutable_slice | undefined-behavior | kernel/infra/src/storage.rs:72:18 | [Undefined Behavior: constructing invalid value of type &mut [u8]: encountered a null reference](stacked/cases.txt#L1446) |
| miri_contract_tests::storage_empty_shared_slice | undefined-behavior | kernel/infra/src/storage.rs:67:18 | [Undefined Behavior: constructing invalid value of type &[u8]: encountered a null reference](stacked/cases.txt#L1472) |
| miri_contract_tests::storage_safe_public_variant_can_own_null | undefined-behavior | kernel/infra/src/storage.rs:80:22 | [Undefined Behavior: constructing invalid value of type std::ptr::NonNull<u8>: at .pointer, encountered 0, but expected something greater or equal to 1](stacked/cases.txt#L1507) |
| miri_contract_tests::storage_uninitialized_bytes_exposed_by_safe_api | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:30:26 | [Undefined Behavior: reading memory at alloc52848[0x0..0x1], but memory is uninitialized at [0x0..0x1], and this operation requires initialized memory](stacked/cases.txt#L1537) |
| miri_contract_tests::storage_zero_size_allocation | undefined-behavior | kernel/infra/src/storage.rs:33:29 | [Undefined Behavior: creating allocation with size 0](stacked/cases.txt#L1566) |
| miri_contract_tests::tinyarc_cell_shared_across_threads | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:80:9 | [Undefined Behavior: Data race detected between (1) non-atomic write on thread `unnamed-2` and (2) retag write of type `i32` on thread `miri_contract_t` at alloc52916](stacked/cases.txt#L1592) |
| rbtree::tests::test_delete_complex | undefined-behavior | kernel/infra/src/rbtree.rs:170:26 | [Undefined Behavior: attempting a write access using <174407> at alloc53159[0x30], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1688) |
| rbtree::tests::test_delete_root_scenarios | undefined-behavior | kernel/infra/src/rbtree.rs:169:39 | [Undefined Behavior: attempting a write access using <174855> at alloc53333[0x28], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1748) |
| rbtree::tests::test_insert_and_search | undefined-behavior | kernel/infra/src/rbtree.rs:169:39 | [Undefined Behavior: attempting a write access using <174595> at alloc53120[0x28], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1808) |
| rbtree::tests::test_iterator | undefined-behavior | kernel/infra/src/rbtree.rs:169:39 | [Undefined Behavior: attempting a write access using <174476> at alloc52959[0x28], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1868) |
| rbtree::tests::test_sequential_insert_stress | undefined-behavior | kernel/infra/src/rbtree.rs:170:26 | [Undefined Behavior: attempting a write access using <175110> at alloc53258[0x30], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L1928) |
| tinyarc::sorted_list::tests::test_basic_functionality | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: trying to retag from <173438> for Unique permission at alloc52965[0x18], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2231) |
| tinyarc::sorted_list::tests::test_find_and_remove | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: trying to retag from <173314> for Unique permission at alloc52938[0x18], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2293) |
| tinyarc::sorted_list::tests::test_task_operations | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: trying to retag from <173314> for Unique permission at alloc52938[0x18], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2346) |
| tinyarc::tests::insert_by_prio | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: trying to retag from <174154> for SharedReadWrite permission at alloc52790[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2399) |
| tinyarc::tests::test_arc_cas_basic | undefined-behavior | kernel/infra/src/tinyarc/mod.rs:584:29 | [Undefined Behavior: trying to retag from <173596> for SharedReadWrite permission at alloc53083[0x0], but that tag only grants SharedReadOnly permission for this location](stacked/cases.txt#L2448) |
| tinyarc::tests::test_detach_and_insert_during_iter | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:124:37 | [Undefined Behavior: trying to retag from <173580> for Unique permission at alloc53030[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2488) |
| tinyarc::tests::test_detach_during_iter | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: trying to retag from <173907> for SharedReadWrite permission at alloc52888[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2535) |
| tinyarc::tests::test_detach_during_iter_2 | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:124:37 | [Undefined Behavior: trying to retag from <173642> for Unique permission at alloc53030[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2580) |
| tinyarc::tests::test_insert_and_detach | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: trying to retag from <173990> for Unique permission at alloc53133[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2645) |
| tinyarc::tests::test_insert_before_and_detach | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:143:52 | [Undefined Behavior: trying to retag from <173707> for Unique permission at alloc53060[0x8], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2694) |
| tinyarc::tests::test_push_and_drop | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:124:37 | [Undefined Behavior: trying to retag from <173541> for Unique permission at alloc53029[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2743) |
| tinyarc::tests::test_push_and_pop | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:124:37 | [Undefined Behavior: trying to retag from <173306> for Unique permission at alloc52927[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2790) |
| tinyarc::tests::test_remove_if | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:124:37 | [Undefined Behavior: trying to retag from <172885> for Unique permission at alloc52755[0x0], but that tag does not exist in the borrow stack for this location](stacked/cases.txt#L2846) |
| tinyarc::tests::test_swap_basic | undefined-behavior | kernel/infra/src/tinyarc/mod.rs:601:29 | [Undefined Behavior: trying to retag from <173344> for SharedReadWrite permission at alloc52991[0x0], but that tag only grants SharedReadOnly permission for this location](stacked/cases.txt#L2893) |
| tinyarc::tests::test_swap_no_none | undefined-behavior | kernel/infra/src/tinyarc/mod.rs:601:29 | [Undefined Behavior: trying to retag from <173466> for SharedReadWrite permission at alloc53036[0x0], but that tag only grants SharedReadOnly permission for this location](stacked/cases.txt#L2924) |

### tree

| 测试 | 分类 | 首个源码位置 | 诊断/日志 |
| --- | --- | --- | --- |
| iheap::tests::concurrent_stress | undefined-behavior | kernel/infra/src/iheap.rs:344:13 | [Undefined Behavior: write access through <237920> at alloc64492[0x30] is forbidden](tree/cases.txt#L1) |
| iheap::tests::fuzz | undefined-behavior | kernel/infra/src/iheap.rs:344:13 | [Undefined Behavior: write access through <172569> at alloc53387[0x50] is forbidden](tree/cases.txt#L58) |
| iheap::tests::fuzz1 | undefined-behavior | kernel/infra/src/iheap.rs:344:13 | [Undefined Behavior: write access through <172272> at alloc53001[0x50] is forbidden](tree/cases.txt#L112) |
| iheap::tests::fuzz2 | undefined-behavior | kernel/infra/src/iheap.rs:344:13 | [Undefined Behavior: write access through <227740> at alloc64065[0x50] is forbidden](tree/cases.txt#L166) |
| iheap::tests::test_basic_insertion | undefined-behavior | kernel/infra/src/iheap.rs:344:13 | [Undefined Behavior: write access through <171564> at alloc53120[0x10] is forbidden](tree/cases.txt#L229) |
| iheap::tests::test_removal | undefined-behavior | kernel/infra/src/iheap.rs:344:13 | [Undefined Behavior: write access through <171814> at alloc53033[0x10] is forbidden](tree/cases.txt#L292) |
| iheap::tests::test_remove_sibling | undefined-behavior | kernel/infra/src/iheap.rs:385:13 | [Undefined Behavior: write access through <172079> at alloc52931[0x10] is forbidden](tree/cases.txt#L346) |
| iheap::tests::test_stack_removal | undefined-behavior | kernel/infra/src/iheap.rs:344:13 | [Undefined Behavior: write access through <171293> at alloc52968[0x10] is forbidden](tree/cases.txt#L400) |
| list::singly_linked_list::tests::test_push | timeout |  | [见日志；超时不等同于 UB](tree/cases.txt#L562) |
| list::typed_atomic_ilist::tests::test_concurrent_ops | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:166:22 | [Undefined Behavior: write access through <177949> at alloc53069[0x10] is forbidden](tree/cases.txt#L577) |
| list::typed_atomic_ilist::tests::test_concurrent_ops1 | undefined-behavior | kernel/infra/src/tinyarc/mod.rs:202:19 | [Undefined Behavior: Data race detected between (1) atomic store on thread `list::typed_ato` and (2) retag read of type `tinyarc::TinyArcInner<list::typed_atomic_ilist::tests::Foo>` on thread `unnamed-2` at alloc53072+0x28](tree/cases.txt#L635) |
| list::typed_atomic_ilist::tests::test_concurrent_ops2 | undefined-behavior | kernel/infra/src/tinyarc/mod.rs:202:19 | [Undefined Behavior: Data race detected between (1) retag read on thread `unnamed-2` and (2) atomic read-modify-write on thread `list::typed_ato` at alloc53147+0x28](tree/cases.txt#L682) |
| list::typed_atomic_ilist::tests::test_insert_and_detach | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:166:22 | [Undefined Behavior: write access through <171192> at alloc52909[0x8] is forbidden](tree/cases.txt#L719) |
| list::typed_ilist::tests::test_insert_and_detach | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:215:17 | [Undefined Behavior: write access through <170875> at alloc52949[0x8] is forbidden](tree/cases.txt#L787) |
| list::typed_ilist::tests::test_insert_before | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:218:17 | [Undefined Behavior: write access through <170738> at alloc52926[0x0] is forbidden](tree/cases.txt#L839) |
| list::typed_ilist::tests::test_list_basic | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:180:17 | [Undefined Behavior: write access through <171169> at alloc53022[0x8] is forbidden](tree/cases.txt#L891) |
| list::typed_ilist::tests::test_list_basic_hole | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:180:17 | [Undefined Behavior: write access through <171136> at alloc53023[0x8] is forbidden](tree/cases.txt#L951) |
| list::typed_ilist::tests::test_list_push_and_pop_several | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:180:17 | [Undefined Behavior: write access through <171011> at alloc52964[0x8] is forbidden](tree/cases.txt#L1011) |
| list::typed_ilist::tests::test_list_push_by_and_pop_several | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:151:42 | [Undefined Behavior: reborrow through <171130> at alloc53061[0x10] is forbidden](tree/cases.txt#L1071) |
| list::typed_ilist::tests::test_list_reorder_chosen_value | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:151:42 | [Undefined Behavior: reborrow through <170853> at alloc52964[0x10] is forbidden](tree/cases.txt#L1119) |
| list::typed_ilist::tests::test_listiterator | leak | kernel/infra/src/list/typed_ilist.rs:528:21 | [memory leaked: alloc52775 (Rust heap, size: 16, align: 8), allocated here:](tree/cases.txt#L1167) |
| list::typed_ilist::tests::test_listreverseiterator | leak | kernel/infra/src/list/typed_ilist.rs:576:25 | [memory leaked: alloc53078 (Rust heap, size: 16, align: 8), allocated here:](tree/cases.txt#L1232) |
| list::typed_ilist::tests::test_safer_insert_and_detach | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:215:17 | [Undefined Behavior: write access through <170155> at alloc52711[0x8] is forbidden](tree/cases.txt#L1283) |
| list::typed_ilist::tests::test_safer_insert_and_detach_several | undefined-behavior | kernel/infra/src/list/typed_ilist.rs:151:42 | [Undefined Behavior: reborrow through <170561> at alloc52898[0x0] is forbidden](tree/cases.txt#L1339) |
| miri_contract_tests::intrusive_nolock_projects_mutable_owner | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:164:26 | [Undefined Behavior: read access through <170490> at alloc52902[0x0] is forbidden](tree/cases.txt#L1383) |
| miri_contract_tests::intrusive_rwlock_projects_mutable_owner | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:146:26 | [Undefined Behavior: read access through <170492> at alloc52904[0x0] is forbidden](tree/cases.txt#L1419) |
| miri_contract_tests::nolock_overlapping_mutable_references | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:103:5 | [Undefined Behavior: write access through <171088> at alloc53040[0x0] is forbidden](tree/cases.txt#L1455) |
| miri_contract_tests::nolock_reader_overlaps_writer | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:114:26 | [Undefined Behavior: read access through <170952> at alloc53017[0x0] is forbidden](tree/cases.txt#L1491) |
| miri_contract_tests::nolock_shared_across_threads | undefined-behavior | kernel/infra/src/nolock.rs:52:18 | [Undefined Behavior: Data race detected between (1) non-atomic write on thread `unnamed-2` and (2) retag read of type `i32` on thread `miri_contract_t` at alloc52943+0x10](tree/cases.txt#L1527) |
| miri_contract_tests::ringbuffer_owned_writer_exposes_uninitialized_bytes | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:63:26 | [Undefined Behavior: reading memory at alloc52651[0x0..0x1], but memory is uninitialized at [0x0..0x1], and this operation requires initialized memory](tree/cases.txt#L1561) |
| miri_contract_tests::ringbuffer_zero_capacity | undefined-behavior | kernel/infra/src/storage.rs:33:29 | [Undefined Behavior: creating allocation with size 0](tree/cases.txt#L1590) |
| miri_contract_tests::storage_empty_mutable_slice | undefined-behavior | kernel/infra/src/storage.rs:72:18 | [Undefined Behavior: constructing invalid value of type &mut [u8]: encountered a null reference](tree/cases.txt#L1627) |
| miri_contract_tests::storage_empty_shared_slice | undefined-behavior | kernel/infra/src/storage.rs:67:18 | [Undefined Behavior: constructing invalid value of type &[u8]: encountered a null reference](tree/cases.txt#L1653) |
| miri_contract_tests::storage_safe_public_variant_can_own_null | undefined-behavior | kernel/infra/src/storage.rs:80:22 | [Undefined Behavior: constructing invalid value of type std::ptr::NonNull<u8>: at .pointer, encountered 0, but expected something greater or equal to 1](tree/cases.txt#L1688) |
| miri_contract_tests::storage_uninitialized_bytes_exposed_by_safe_api | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:30:26 | [Undefined Behavior: reading memory at alloc52848[0x0..0x1], but memory is uninitialized at [0x0..0x1], and this operation requires initialized memory](tree/cases.txt#L1718) |
| miri_contract_tests::storage_zero_size_allocation | undefined-behavior | kernel/infra/src/storage.rs:33:29 | [Undefined Behavior: creating allocation with size 0](tree/cases.txt#L1747) |
| miri_contract_tests::tinyarc_cell_shared_across_threads | undefined-behavior | kernel/infra/src/miri_contract_tests.rs:80:9 | [Undefined Behavior: Data race detected between (1) non-atomic write on thread `unnamed-2` and (2) retag read of type `i32` on thread `miri_contract_t` at alloc52916](tree/cases.txt#L1773) |
| tinyarc::sorted_list::tests::test_basic_functionality | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: reborrow through <170887> at alloc52965[0x18] is forbidden](tree/cases.txt#L2277) |
| tinyarc::sorted_list::tests::test_find_and_remove | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: reborrow through <170768> at alloc52938[0x18] is forbidden](tree/cases.txt#L2339) |
| tinyarc::sorted_list::tests::test_task_operations | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: reborrow through <170768> at alloc52938[0x18] is forbidden](tree/cases.txt#L2392) |
| tinyarc::tests::insert_by_prio | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: reborrow through <171584> at alloc52790[0x8] is forbidden](tree/cases.txt#L2445) |
| tinyarc::tests::test_arc_cas_basic | undefined-behavior | kernel/infra/src/tinyarc/mod.rs:588:15 | [Undefined Behavior: write access through <171091> at alloc53083[0x0] is forbidden](tree/cases.txt#L2491) |
| tinyarc::tests::test_detach_and_insert_during_iter | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:145:22 | [Undefined Behavior: write access through <171411> at alloc53030[0x8] is forbidden](tree/cases.txt#L2538) |
| tinyarc::tests::test_detach_during_iter | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: reborrow through <171353> at alloc52888[0x8] is forbidden](tree/cases.txt#L2592) |
| tinyarc::tests::test_detach_during_iter_2 | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:145:22 | [Undefined Behavior: write access through <171433> at alloc53030[0x8] is forbidden](tree/cases.txt#L2634) |
| tinyarc::tests::test_insert_and_detach | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:122:38 | [Undefined Behavior: reborrow through <171429> at alloc53133[0x0] is forbidden](tree/cases.txt#L2706) |
| tinyarc::tests::test_insert_before_and_detach | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:145:22 | [Undefined Behavior: write access through <171528> at alloc53060[0x8] is forbidden](tree/cases.txt#L2755) |
| tinyarc::tests::test_push_and_drop | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:145:22 | [Undefined Behavior: write access through <171389> at alloc53029[0x8] is forbidden](tree/cases.txt#L2816) |
| tinyarc::tests::test_push_and_pop | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:145:22 | [Undefined Behavior: write access through <171157> at alloc52927[0x8] is forbidden](tree/cases.txt#L2870) |
| tinyarc::tests::test_remove_if | undefined-behavior | kernel/infra/src/list/typed_atomic_ilist.rs:145:22 | [Undefined Behavior: write access through <171367> at alloc52755[0x8] is forbidden](tree/cases.txt#L2933) |
| tinyarc::tests::test_swap_basic | undefined-behavior | kernel/infra/src/tinyarc/mod.rs:603:23 | [Undefined Behavior: write access through <170833> at alloc52991[0x0] is forbidden](tree/cases.txt#L2987) |
| tinyarc::tests::test_swap_no_none | undefined-behavior | kernel/infra/src/tinyarc/mod.rs:603:23 | [Undefined Behavior: write access through <170955> at alloc53036[0x0] is forbidden](tree/cases.txt#L3025) |
