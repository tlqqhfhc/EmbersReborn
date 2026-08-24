# 搜打撤核心循环 — 实现计划

> 状态：待用户确认后执行。项目代码地图见 `.trae/rules/project_rules.md`（执行中保持同步更新）。

## 目标循环

lobby 合成装备 → E 交互 Gateway 进地图 → 平面地图战斗（creeper + zombie）/拾取物资 → 进图 90s 后传送门出现（常驻）→ E 交互传送门回 lobby。死亡 = 携带物品全丢 + 回 lobby 满血重生。

## 已确认的设计决策

1. **材料来源**：地图带回来（怪物掉落 + 地面散落材料 `embers:ember_shard`）；仅首次进入 lobby 赠送初始 8 片，之后不再赠送。
2. **死亡惩罚**：背包全清，回 lobby 满血重生（无死亡 UI，第二阶段再做）。
3. **传送门**：进入地图后固定 90s（代码常量，可调）出现，常驻不消失。更有趣的规则后续研究。
4. **交互入口**：纯实体交互。lobby Gateway（黑立方）E → 进地图；地图传送门 E → 回 lobby；合成站 E → 合成面板。**不做 GatewayMenu**。
5. **地图**：平面 + 手工布点障碍物 + 随机刷新怪物（creeper ×3 + zombie ×5，位置随机、避开入口）。地形生成/建模属第二阶段。
6. **Zombie 新怪物**：近战 AI；生成时随机持武器（sword / spear / 空手）；攻击**复用玩家 melee 扇形伤害逻辑**（把 melee 模板核心抽成共享函数）；空手 = 小幅直伤。

## 范围外（第二阶段）

UI 缺陷（选项子页/暂停屏）、FPS 视角与移动方案改造、地形生成/手工建模、音频、体素世界（方块破坏/放置）、存档、更有趣的传送门规则、Creeper 模型/表现。

---

## P1 维度传送机制（核心地基）

**验收**：lobby E 黑立方 → 加载屏 → 进地图；地图 E 传送门 → 回 lobby；背包物品跨维度保留。

1. **按维度键分发场景**（`dim.rs::handle_dimension_generation_request`）
   - 拆出 `lobby_scene()`（现有内容改造：去掉 creeper/地面 sword+tnt 物品，保留 dummy 测试桩；Gateway 改为指向 operation；加合成站占位（P4 补交互））与 `operation_scene()`（P2 填内容，P1 先放空地面+传送门生成逻辑）。
   - 新维度键：`embers::OPERATION`（`embers:operation`）。
2. **双向 Gateway**
   - `gateway(target_dimension)` 场景工厂带参；注册两个交互：`embers:gateway_to_operation`、`embers:gateway_to_lobby`（替换现有 `gateway_travel` 开菜单行为），on_begin 触发 `Load::EnterDimension(GatewayTravel, target)`。
3. **GatewayTravel 任务图**（替换 `loading_screen.rs` L247-249 的 `todo!()`）
   - 任务链（叶子→根）：`FetchPayloadScopeTask(Dimension(operation))` → `ReloadMetadataTask` → `DimensionGenerationTask(target)` → `PlayerMigrationTask(target)`。
   - `PlayerMigrationTask::task()` = `trigger(PlayerMigrationRequest { to })`（Command 里做不了查询，迁移逻辑放 observer）。
4. **玩家迁移 observer**（新增，`dim/` 下新模块如 `player_migration.rs` 或放 player.rs）
   - 步骤（顺序敏感，关键坑：物品实体是玩家子实体，玩家随维度 despawn 会连带消失，必须先 re-parent）：
     1. 找旧玩家（旧维度 `With<Player>`），快照 `PlayerInventory` 38 槽 `Vec<Option<Entity>>`；
     2. 存活物品实体 re-parent 到临时 limbo 实体（ChildOf RootNode）；
     3. 旧维度：移除 `ActiveDimension`、`LoadedDimensions` 删旧键、`despawn()` 整棵子树；
     4. 新玩家（新维度 `With<Player>`）：槽位写入迁移来的实体、re-parent 回新玩家、`SpawnPoint` 设为新维度入口坐标；
     5. despawn limbo。
   - 边界：死亡重生时旧玩家可能已不存在 → 迁移按空背包处理（P3 依赖此分支）。
   - 注意：loading 任务图作者自注 "Faulty logic"，新增任务先小步验证；确有 bug 则做最小修复（属核心流程，不在"UI 暂缓"范围）。

## P2 战斗地图 + Zombie

**验收**：地图有障碍物与散落实体物资；creeper/zombie 随机刷新并可被击杀；击杀掉落 ember_shard；90s 后传送门出现。

1. **operation 场景**（`operation_scene()`）
   - 40×40 平面（复用 lobby 的 Plane3d + heightfield 模式）；若干 cuboid 障碍（Mesh3d + Environment collider）。
   - 玩家入口点（固定，如边缘 (0, 1, 18)）；怪物随机刷点区域（避开入口半径 6）。
   - 散落实体物资：N=10 个 item_actor（8×ember_shard + 2×随机武器），随机位置。
2. **怪物刷新**：场景生成时 `SystemRng` 随机位置 spawn creeper ×3 + zombie ×5（数量/位置常量集中一处便于调参）。
3. **传送门计时**
   - 新组件 `PortalTimer(Timer 90s)` 挂在 operation 维度实体（或维度场景根）；系统（Dimension 状态 + 当前维度=operation）tick，到期 spawn `gateway(lobby)`（位置在入口附近或地图中心，定一个合理点）。
   - 不用 `WorldTime`（语义是"一天时刻"，不混用）。
4. **Zombie**（新文件 `dim/actor/living/zombie.rs` + `zombie.actor.toml`，hp 30、speed 与玩家相当）
   - 状态机 `ZombieState { Idle, Chase, Attack }`，复用 `perceive_targets`/`chase`/`AiPerception`/`HitStun`。
   - 组件 `ZombieWeapon(Option<NamespacedKey>)`：spawn 时随机 sword/spear/None（比例常量，如 4/3/3）。
   - 攻击：进 attack_range 且 `AttackCooldown`（现成组件，终于有使用者了）就绪 → 共享近战函数 → 重置冷却。
   - 模型：占位（绿色圆柱/简单盒体组合，与玩家圆柱区分）。
   - `LootTable`：ember_shard ×2。
5. **melee 共享化**（改 `dim/item.rs`）
   - 把 melee 模板 `on_end` 的扇形范围伤害逻辑抽成共享函数 `melee_strike(attacker, facing, config)`；玩家 melee 模板与 zombie 攻击系统都调它。
   - 空手 zombie：直接发 `Damage`（2-4 点，径向击退，`DamageSource` 填 zombie）。
6. **LootTable 小幅增强**（`living.rs` 死亡掉落处）
   - 支持表内重复条目 = 掉落多件（现有"逐条生成 item_actor"行为已满足，只需数据层约定）；随机性先不做。

## P3 死亡惩罚

**验收**：地图死亡 → 背包清空 → 回 lobby 满血；lobby 死亡（理论上无怪，兜底）→ 原地满血重生。

1. 改 `damage` 系统玩家死亡分支（`living.rs`）：
   - 不再"满血 + 瞬移 SpawnPoint"，改为：
     1. 清空 `PlayerInventory`（38 槽物品实体直接 despawn = 全丢）；
     2. 若当前维度 ≠ lobby：trigger `Load::EnterDimension(GatewayTravel, lobby)`（走 P1 迁移，空背包分支）；
     3. 若当前维度 = lobby：玩家实体保留，回满血 + 重置到 SpawnPoint（兜底）。
   - 死亡瞬间先满血再传送（避免新玩家 Health 计算异常——Health 由 MaxHealth 属性初始化，重新生成即可）。
2. 时序注意：死亡发生在本帧，迁移在加载任务图里跨帧完成 → 加载屏期间无玩家实体，HUD/相机系统（`Single<Player>`）要能容忍短暂无玩家（确认 `Single` 用 Option 或保证迁移先于 overlay 恢复；实现时验证，必要时给相关系统加存在性门控）。

## P4 合成系统

**验收**：lobby E 合成站 → 合成面板 → 消耗 ember_shard 合成 sword/spear/tnt 进背包。

1. **材料物品** `embers:ember_shard`：`ember_shard.item.toml`（max_stack_size=64，weight=0.05）+ 纹理（MVP 用 missingno 兜底）。
2. **配方表**（MVP 代码常量，集中一处；跑通后再考虑 toml 数据化）：
   - 8×shard → sword；12×shard → spear；4×shard → tnt。
3. **合成站实体**（lobby 场景）：小桌（Box mesh + collider）+ `Interactable` → 新交互键 `embers:crafting_open`：on_begin 切 `ActiveOverlay::Crafting`。
4. **ActiveOverlay 加 `Crafting` 变体**（`ui.rs`）：`process_escaping` 加 `Crafting → HeadsUpDisplay` 分支；新 overlay 值加入回退栈。
5. **最小合成 UI**（新文件 `ui/crafting.rs`，挂 DimensionViewNode）：
   - 每配方一行：材料图标+数量 → 箭头 → 产物图标+名称 → Craft 按钮。
   - 按钮点击：数全背包材料（遍历 38 槽 StackCount）→ 足够则逐槽消耗 → `move_item`/insert 产物入背包；不足则按钮禁用（灰态）。
   - `Changed<Inventory>` 刷新可用状态；复用现有 `text_button` + `item_image_node` 工厂。
6. **物品增删辅助**：背包内"数 N 个某物品""消耗 N 个"辅助函数（基于 inv.rs 现有结构，注意堆叠拆分）。

## P5 初始物资 + 端到端验证

1. 首次进 lobby（`Init` 流程）：新玩家背包预置 8×ember_shard（P1 之后其他进 lobby 路径不给）。
2. 地图散落/掉落数值过一遍（掉率、数量、90s 时限、怪物数量），集中成常量表。
3. 验证：`cargo fmt` → `cargo clippy --workspace --all-targets --features dev` → `cargo test --workspace --all-targets --features dev` → `cargo build`。
4. GUI 无法自动测试 → 交付用户手动跑一遍完整循环（lobby 合成 → 进图 → 战斗/拾取 → 90s 传送门 → 回 lobby → 死亡惩罚路径）。
5. 更新 `.trae/rules/project_rules.md`（新文件/新组件/已解决缺口）。

## 风险与备注

- **最大风险 = P1 玩家迁移**（实体父子关系 + 库存指针 + 加载任务图时序），建议 P1 单独做完先验证往返传送再进 P2。
- P1 会动 `loading_screen.rs`（自述有逻辑缺陷的加载 DAG），可能顺带最小修复其 bug。
- melee 共享化（P2-5）会动 `item.rs` 模板结构，注意保持玩家侧行为不变。
- 所有新数值（时限/数量/配方/掉落）集中为常量，便于第二阶段调参。
- 每完成一个 P 阶段：跑 lint/test + 更新项目地图 + 让用户实测。
