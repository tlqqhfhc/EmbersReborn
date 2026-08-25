# Embers 项目地图（AI 快速定位文件职责，避免重复读文件）

> 本文件是项目代码地图。读代码前先看这里；读完重要新文件后应更新本文件，注意不要出现幻觉。
> 状态标记：✅ 已实现 | ⚠️ 占位/未实现/停用 | 💥 运行时会 panic

## 项目概览
- Rust + Bevy 0.19 的 Minecraft 风格 3D 体素游戏（edition 2024）。workspace = `embers`（主游戏）+ `embers-macros`（proc macro）。
- 物理：avian3d 0.7；角色运动：bevy-tnua（动作框架，Knockback/Dash/Crouch/Walk 皆为"动作"）；粒子：bevy_sprinkles。
- 资产双根：`pld/`（原始资产，标记文件 `.embers_payload_root`）→ `shp/`（处理后资产，`.embers_shipment_root`）。main.rs 从 exe 向上查找；运行时只读 shp（AssetMode::Processed）。
- **pld→shp 是手动镜像，无自动同步**：shp 整体被 gitignore（`.gitignore: shp/**`），是本地运行目录。新增/修改 pld 资产（toml/png/glb…）必须**手动复制到 shp 对应相对路径**，否则运行时 `Couldn't find payload of type X in embers:<key>` / 资产缺失（P2 实测：zombie.actor.toml 没复制进 shp → 5 个 zombie 场景构建失败）。`.meta` 可缺省，bevy 自行处理。
- 数据驱动：`pld/` 下 TOML 定义在运行时编译成 Bevy 资产（见 pld/def.rs），支持命名空间覆盖（后挂载源/作用域优先）。
- CI（.github/workflows/ci.yml）：fmt → clippy+test（dev features，Win+Linux）→ clippy+release build+上传产物。
- Git 工作流（.trae/rules/git_workflow.md）：小步提交、及时 `git push origin main`、出错优先回滚不叠补丁。
- 已知死依赖：`bevy_voxel_world 0.17` 在 Cargo.toml 声明但源码零使用。

## embers/src 文件职责

### main.rs
入口。定位 pld/shp 根；插件注册：DefaultPlugins(Asset Plugin 双根) + DirectionalNavigation + PhysicsPlugins(带 SourceExclusionCollisionHooks) + Sprinkles + TabNavigation + TnuaControllerPlugin::<Movements>(PhysicsSchedule) + TnuaAvian3dPlugin + dim/input/pld/ui 插件。Startup 触发 `Load::EnterMainMenu(Init)`。窗口初始 visible=false（等 icon 加载）。

### utils.rs
`path!` 宏（跨平台路径拼接）；`NamespacedKey`（`namespace:key`，正则校验，`new_embers()` 构造 embers 命名空间）；trait：`Keyed`/`Namespaced`/`UniquelyIdentified`/`TypeKey`/`Marker`/`DynPartialCmp`；`SystemRng<R: SeedableRng>`（`#[derive(Resource)]`，P2 显式加——bevy 0.19 Resource 须显式 derive；实例 `SystemRng<SmallRng>` 在 living::plugin `init_resource`，系统里用 `ResMut<SystemRng<SmallRng>>`）；`TextureAtlasManifest`；bsn 模板辅助（`template_bundle`/`template_bundle_for`/`remove_bundle`）。含 NamespacedKey 单元测试。

### utils/physics.rs
只有一个函数 `section(arc_rad, radius, height) -> Option<Collider>`：扇形碰撞体（递归二分 + convex_hull 逼近），供 melee 动作的范围判定用。

### input.rs
`InputButton`（KeyCode|MouseButton 统一枚举）；`button_input!` 宏族（pressed/just_pressed/... 按绑定分派，绑定是 `RwLock<InputButton>` 静态量，见 player.rs 的 `controls!` 宏）；`DoubleClicks` 资源（227ms 阈值双击状态机：double_clicked/just_started/just_ended）；`InteractionTrigger`（Click/DoubleClick）。PreUpdate 统一 tick。含单元测试。

### pld.rs
`Payload` trait（`payload_root()` 声明资产根目录）；`Boxed<M,T>`（trait-object 型资产，泛型擦除）；`Tag<A>`（资产路径集合，用于动作打断匹配，root=`resolved_tags`）；`Payloads::inject`（UUIDv5 内存注入资产）；`PayloadPath`；为各类内置资产（Image/Gltf/Font/Shader/ParticlesAsset/TextureAtlasLayout/AudioSource 等）实现 Payload。嵌入式资产：icon.png、missingno.png。

### pld/manager.rs
`PayloadManager`：核心资产解析器。作用域栈（`Global`→`global/`，`Dimension(key)`→`dim/embers/<key>/`）× 数据源列表（每源一个 UUID，embers 内置源固定 UUID）。事件：Fetch/EvictPayloadScopeRequest、Mount/UnmountPayloadScopeRequest（动态挂载整套数据源，为 mod/多人预留）、RefetchPayloadRequest（热重载）。`resolve_handle`/`resolve_payload`：源倒序×作用域倒序查找（覆盖机制），先 AssetServer 文件夹再内存注入。`inject_keyed_embers_payload_batch`：PreStartup 批量注入代码构建的 payload。
`monitor_folder_loads`：等待作用域文件夹加载完成（`loading_scopes` 清空时触发 `PayloadFetchingComplete`）。**已修（P1）**：bevy 0.19 在文件夹加载**失败**（目录不存在）或**重复加载已加载文件夹**时都不发 `LoadedWithDependencies` 事件，原实现会永久卡死加载（如进无资产目录的 operation 维度、或二次回 lobby）。现额外轮询 `get_load_states`：文件夹 `Failed`→当空作用域（warn）；`Loaded` 且递归依赖到终态→当完成。这样"无目录的维度"和"重复进入已加载维度"都能正常推进。

### pld/def.rs
TOML 定义 → 运行时资产的编译管线。`compile_definitions` 泛型系统 + `RecompileDefinitionsRequest` 事件（整体重编译）。
- `ActorDef`（`*.actor.toml`）✅：float_height + attributes 表 → `MovementsConfig`（tnua 参数）+ `AttributeBase`（属性基线）。
- `ItemDef`（`*.item.toml`）✅：顶层键 → 物品组件原型（`inject_prototype`）。
- `ItemActionDef`（`*.item_action.toml`）✅：template(melee/throw/charged_throw) + config → `ItemAction`。
- `BlockDef`（`*.block.toml`）⚠️：有 loader 无 observer，`recompile_blocks()` 整段注释，**管线停用**。
- `TagDef` ⚠️：`// TODO Tag resolution` 未实现。
其他 loader：`.atlas.toml`（网格图集布局）、`RawAssetLoader`（泛型 toml→serde）。

### pld/foundry.rs
资产引用/场景模板工厂。`PayloadTemplate`（Handle/Path/Value 三态，实体生成时解析）；`pld`/`optional_pld`/`payload_value` 辅助（Image 缺失 fallback missingno.png）；glTF 模板（`GltfElement` 按 index/名取 scene/mesh/material/node/skin，`GltfElementTemplate`，`animate_actor` 动画图）；`rich_image`（png+atlas.toml+animation.toml+scaling.toml 四元组 → ImageNodeTemplate + 可选 AnimatedTexture）；`ui_image_node`/`item_image_node`/`block_texture`/`text_font`/`default_scene`/`actor_scene`。

### dim.rs
维度与全局战斗框架：
- `Dimension(key)`/`ActiveDimension`/`LoadedDimensions`/`DimensionGenerationRequest`。
- 维度场景按 key 分发（P1 ✅）：`handle_dimension_generation_request` 先 despawn 旧维度（despawn 命令先入队、spawn_scene 后入队，flush 顺序保证 `ActiveDimension` 唯一，`ActorTemplate` 的 single 查询才安全）→ 按 key 生成：
  - `lobby_scene`：20×20 白色地面（Plane3d+heightfield）+ `dimension_barrier(20.)` + gateway→operation（0,0,-5）+ dummy（5,0.5,0）。
  - `dimension_barrier(size)`（空气墙，防掉落/穿墙 ✅）：四面静态 Environment 层墙围住 `size×size` 地面（±X/±Z 边），厚 1.0、高 5.0，半透明蓝；**±Z 墙加长 `2×THICKNESS` 覆盖两角，无缝隙**；半透明 `srgba(0.3,0.4,0.8,0.18)`。碰撞体与网格**全尺寸一致**（`Collider::cuboid(size+2, 5, 1)` / `Collider::cuboid(1, 5, size)`，见下方"全尺寸"坑）。lobby/operation 各挂一组（20./40.）。
  - `operation_scene`（P2 ✅）：40×40 地面 + `dimension_barrier(40.)` + 7 个 cuboid 障碍（`OPERATION_OBSTACLES` 常量表 (center,size)，`obstacle()` 场景 = Mesh3d cuboid + 棕灰 StandardMaterial + `{ PhysicsPreset::Environment.physics(false) }` + 显式 `Collider::cuboid(全尺寸)`）+ 根实体挂 **PortalTimer（90s Once）**。**P2 移除了初始 gateway**——operation 内的传送门只在 90s 后由 PortalTimer 生成。
  - `unknown_dimension_scene`：最小场景（光+20×20 地面）+ warn 日志。
  - `dimension_spawn_point`：lobby=(0,1,0)、operation=(0,1,18)、其他=(0,1,0)。
- **operation 内容随机刷取（P2 ✅）**：`handle_dimension_generation_request` 在 key=OPERATION 生成后调 `spawn_operation_content(commands, &mut rng)`（参数 `ResMut<SystemRng<SmallRng>>`）：`creeper()`×3 + `zombie(ZombieWeapon::roll(rng))`×5 + `item_actor_of(item_stack(key))`×10（8×EMBER_SHARD + 2×SWORD/SPEAR 各半随机），各自 `template_value(Transform::from_translation(random_operation_position(rng)))`。`random_operation_position`：±16 边界（OPERATION_SPAWN_BOUND）、y=1、距入口 (0,1,18) ≥6（ENTRY_CLEAR_RADIUS）、16 次重试、fallback (0,1,-16)。常量：CREEPER_SPAWN_COUNT=3、ZOMBIE_SPAWN_COUNT=5、SCATTERED_SHARD_COUNT=8、SCATTERED_WEAPON_COUNT=2、OPERATION_PORTAL_DELAY_SECS=90、OPERATION_PORTAL_SPAWN=(0,0.5,0)。
- **PortalTimer（P2 ✅）**：`pub struct PortalTimer(Timer)`（Component，挂 operation 根）；`tick_portal_timers`（Update，run_if `GameState::Dimension`）对 `(Entity, &mut PortalTimer), (With<Dimension>, With<ActiveDimension>)` tick 定时器，`just_finished()`（Once 模式只触发一帧）时 spawn `gateway(&INTERACTION_GATEWAY_TO_LOBBY)` 于 (0,0.5,0) 并 ChildOf 维度根——**传送门常驻**（不消失；维度 despawn 时随根实体一起销毁）。
- **玩家与维度子树解耦（P1 ✅）**：玩家 ChildOf `RootNode`（不在维度实体下）；维度生成时若玩家已存在→瞬移（Transform+SpawnPoint+LinearVelocity 清零，背包物品 ChildOf 玩家自然保留），不存在→`player_scene` 新建。
- 物理：`CollisionLayer`（Interactable/LivingActor/MiscActor/Phantom/Projectile/Environment）+ `PhysicsPreset`（dominance、轴锁定、**仅 Environment 预设是 `RigidBody::Static`，其余恒为 `Dynamic`**）+ `SourceExclusionCollisionHooks`（碰撞源排除，SparseSet，有 benchmark TODO）。
- **avian3d 0.7 物理坑（实测）**：① 默认特性 `collider-from-mesh`/`default-collider` 已开，但 **Mesh3d 本身不产生 collider**，须显式加 `Collider` 或 `ColliderConstructor`（现有 actor/item_actor 都是显式 `Collider::cylinder/cuboid`）。② `Dynamic` 刚体即便无 collider 也受重力 → 会掉穿地面（gateway 曾因此掉落，改 Static 解决）。③ E 键交互用 `SpatialQuery::shape_intercepts` 查 `Interactable` 层 collider，实体必须有挂 Interactable 层的 collider 才能被检测到。④ bsn! 里覆盖 preset 的 RigidBody 用 `template_value(RigidBody::Static)`（裸 `RigidBody::Static` 会走 patch 机制报 `default_static` 错）；后写的组件覆盖先写（含嵌套场景的）。⑤ **`Collider::cuboid(x,y,z)` 接收全尺寸**（内部 `*0.5`，见 parry/mod.rs），与网格 `Cuboid::new(全尺寸)` 同语义；**传半尺寸 → 碰撞体只有视觉一半**（空气墙穿墙 bug 真根因：加厚版墙碰撞体只覆盖中央段，40m 地图角落 (±20,±20) 无碰撞体可直接走出去；obstacle 同款 bug 已修）。⑥ **PhysicsLayer 宏位分配**：`#[default]` variant 被提到声明顺序最前，再按（调整后）顺序从 bit 0 分配 `1<<i`。本项目实际 bit：**Phantom=1、Interactable=2、LivingActor=4、MiscActor=8、Projectile=16、Environment=32**（Phantom 是 `#[default]`；调 enum 顺序/挪 `#[default]` 会整体改 bit 值，勿假设）。⑦ **spawn 时 `CollisionLayers` 必须写在 `Collider` 之前**（同实体内）：collider_tree 的 Case 9 observer 只响应 CollisionLayers 的 Insert 更新 proxy layers；Collider 先插、CollisionLayers 后插时 proxy 保持默认 (1, ALL) → 碰撞对不上（实测球穿一切）。
- `Movements`（Tnua scheme：Knockback/Sneak/Roll，basis=TnuaBuiltinWalk）；`MovementsConfig` payload（per-actor 移动参数，root=`movements_configs`）。
- **Action 框架**：`Action` trait（on_begin/on_end→Option<next_action_key>/duration）、`ActionSlots`（click/double_click 两槽）、`ActionStatus`（Idle/Active+Stopwatch）、泛型系统 `update_action`（动作执行+链条+`ActionInterruption` 打断，经 Tag 匹配）。
- `EntityInteraction` 资产（root=`entity_interactions`）：E 键实体交互，PreStartup 注入三个内置：`item_actor/pickup`（200ms，物品移入玩家背包）、`gateway_to_lobby`/`gateway_to_operation`（200ms，直接触发 `Load::EnterDimension(GatewayTravel, 目标维度)`，不开菜单）。
- `Interactable` 组件（distance_factor、initial_click/double_click 键）。
- `Explosion` 事件 + `explode` observer：一次爆炸粒子（bevy_sprinkles）+ 球形范围（radius=power*2）LivingActor 查询，衰减伤害 `7*(x²+x)*power+1` + 方向击退 17*x。`// TODO consider more realistic explosions`（不破坏方块、不连锁）。
- `WorldTime(f32)` 组件（0..1 一天中的时刻，默认 0.25，**无驱动系统**）。
- `gateway(interaction: &NamespacedKey)` 场景（3×1×3 黑立方，**unlit 黑色剪影**（不依赖光照，白色地面上可见）+ `PhysicsPreset::Phantom.physics(true)` 后 **`template_value(RigidBody::Static)` 覆盖为静态刚体**（portal 不该受重力，否则掉穿地面）+ **显式 `Collider::cuboid(1.5,0.5,1.5)`**（E 键靠 SpatialQuery 查 Interactable 层 collider，无 collider 则检测不到）+ Interactable 绑定传入交互键，P1 参数化）；lobby 实例 (0,0.5,-5)；operation 实例 P2 移除（改为 PortalTimer 90s 后在 (0,0.5,0) 生成）。`INTERACTION_GATEWAY_TO_LOBBY`("embers:gateway_to_lobby")、`INTERACTION_GATEWAY_TO_OPERATION`("embers:gateway_to_operation")。
- `embers::ASSEMBLY_APEX`("embers:assembly_apex")、`embers::LOBBY`("embers:lobby")、`embers::OPERATION`("embers:operation") 维度键常量。

### dim/block.rs
`Block(key)` 方块标识；`BlockCollider([u64;8])`（8×8×8 体素位打包）；`BlockModel`（2×2×2 体素 u8 掩码 + 未使用的 extra mesh）；`BlockColliderTemplate`/`BlockVoxelModelTemplate` trait。⚠️ **`plugin()`（L90-223）整段被注释**——5 种 collider 模板（empty/full/slab/layered/custom）+ 4 种 voxel model 模板（empty/cube/cube_all/bottom_slab）全部未生效。

### dim/chunk.rs
`Chunk { blocks: [Block; 16^3] }` 组件（on_insert/on_remove 钩子）。⚠️ 插入钩子整段注释（原设计：voxel 碰撞体 + GreedyChunkMesher 网格 + 材质）；移除钩子有效（删 Collider/Mesh3d/材质）。`ChunkMeshData` 空结构；`GreedyChunkMesher::generate_mesh` 💥 `todo!()`。**体素世界完全未实现**（无区块生成/加载/渲染/破坏/放置）。

### dim/item.rs
`ItemStack(key)` + `StackCount(u8)`；`item_stack(key)` 场景辅助。
- **动态物品组件系统**：`ItemComponentType` trait + `Boxed` payload 注册表（root=`item_components`）+ `StandardItemComponentType<C>`（toml 值→原型资产→实体化时注入）。已注册 5 种：`Enchantments()`⚠️空、`InitialItemActions`（hands/armor × click/double_click 动作键）、`MaxStackSize(u8)`、`RangedAmmo()`⚠️空、`Weight(f32)`。
- `ItemAction` 资产（root=`item_actions`）：on_begin/on_end 闭包 + wield（Hands-Single/Hands-Dual/Armor）+ duration。
- `attacker(holders, transforms, item)`：按 ChildOf 解析物品持有者实体 + GlobalTransform（作者 WIP 重构，替代原硬编码玩家 Single 查询）→ melee/throw 模板持有者无关，**mob 持武器可复用玩家攻击**（P2 zombie 依赖此）。
- `MeleeStrike { collider, damage, knockback }`（P2 ✅ 共享近战打击）：`new(damage, knockback, arc_deg, range) -> Option<Self>` 预计算 `utils::physics::section` 扇形 collider；`apply(commands, spatial_query, attacker, transform)` = LivingActor 层 `shape_intersections`（排除 attacker）+ 批量 `Damage`（径向击退）。**扇形朝向修正**：section collider 以本地 **+X** 为中心，Tnua 角色朝向本地 **-Z** → 查询旋转 `transform.rotation() * Quat::from_rotation_y(FRAC_PI_2)`（+90°Y 把 +X 映射到 -Z），否则挥击偏 90°。此修正同时改变了玩家攻击方向（P2 前偏 90°，用户实测确认）。
- 内置 3 个模板（`BoxedItemActionBuilder`，PreStartup 注入）：`melee`（on_end 经 `attacker()` 解析持有者后用 `MeleeStrike` 打击；duration 为 None 时执行）；`throw`（spawn primed_tnt，初速沿持有者 -Z）；`charged_throw`⚠️（快速松开链到 hold_action，蓄力后只 `println!("throwing")`，**无实际投掷**）。
- embers 物品键：`SWORD`/`SPEAR`/`TNT`/`EMBER_SHARD`（P2 加）。

### dim/item/inv.rs
`Inventory<N, M>`（`[Option<Entity>; N]` 槽位，M=持有者 marker；物品实体 ChildOf 持有者）。Index/IndexMut（单槽+6 种 range）。
`ItemSource`/`ItemDestination`（各 3 种：背包槽区间/背包单槽/世界 ItemActor）；`ItemMoveQuantity`（All/Half/One）。
`Commands::move_item`（MoveItemCommandExt）：单槽↔单槽（`try_stack` 堆叠=同 key+所有动态组件 dyn_eq+MaxStackSize，否则交换/移空槽）、区间→单点、单点→区间。💥 **区间→区间 = `unimplemented!()`（L450）**。

### dim/actor.rs
`Actor` 标记组件（require Transform）；`ActorTemplate`（按维度键从 LoadedDimensions 找维度实体，挂 ChildOf；找不到报 NonexistentDimensionError）；`actor()` 基础场景；plugin：注册 `primed_tnt::fuse`（仅 Dimension 状态）+ living 子插件。

### dim/actor/living.rs
`LivingActor` 标记；`Health(f32)`（按 MaxHealth 属性初始化）。
`living_actor(key, interactable)` 通用生物场景 = actor + 物理预设 + AttributesTemplate + Health + TnuaController<Movements> + MovementConfigTemplate（按 actor key 解析 MovementsConfig payload）。
消息：`Damage { target, amount, knockback, source }`、`DamageNumber { position, amount }`；`DamageKnockback`（Directional/Radial 默认 20/None）；`DamageSource { origin, causing_entity, direct_entity }`（⚠️ 后两者在 damage 系统里被丢弃，无仇恨归因）。
`damage` 系统：属性减免（DamageTaken.value_for）→ 扣血 → DamageNumber 飘字（头顶 +1.5）→ 击退（Tnua Knockback 动作，可打断当前动作，受 KnockbackTaken 属性）→ HitStun 0.25s → 死亡：**玩家（P3 ✅ 搜打撤惩罚）=回满血+清空击退动作；若当前在非 lobby 维度且非 LoadingScreen 态→清空背包（全槽 despawn）+ `Load::EnterDimension(PortalTravel, LOBBY)` 传送回 lobby（维度生成把玩家放到 lobby 出生点）；否则（lobby 内/加载态）=瞬移 SpawnPoint 兜底；怪物=按 LootTable 在死亡点 +0.5Y 生成 item_actor 后 despawn**（LootTable 每项一个 item_actor，重复条目=多件掉落）。
plugin：`init_resource::<SystemRng<SmallRng>>()` + Update `(damage, creeper::system, zombie::system).run_if(in_state(Dimension))` + ai/damage_number 子插件。

### dim/actor/living/player.rs
`controls!` 宏生成可重绑定静态绑定（`RwLock<InputButton>`）：左键移动、Space 翻滚、E 交互、ShiftLeft 主手、鼠标右键副手、T 护甲、1-6 快捷栏、F 换副手、R 背包。
`Player` 组件（require：SelectedHotbarSlot、PlayerEntityInteractionStatus/Slots、PlayerItemActionStatus/Slots、PlayerInventory、SpawnPoint）；`PlayerInventory = Inventory<38>`（0-6 快捷栏、36 护甲、37 主手、7-35 主背包）；`EquipmentSlot`（MainHand/OffHand/Armor）；`SpawnPoint`（默认 (0,1,0)）；`OffHandSwapped` 消息。
5 个输入系统（子调度，全部要求 GameState::Dimension；移动/交互/物品动作另要求 overlay=HeadsUpDisplay）：
- `process_input_movement`：**按住鼠标左键朝光标屏幕位置移动**（RTS 式，非 FPS 视角）；翻滚同向。
- `process_input_entity_interactions_schedule`：E 单击/双击 → 玩家为中心 r3 h2 圆柱 SpatialQuery 命中 Interactable 层，按 dist×distance_factor 取最近 → 解析 initial_click/double_click 的 EntityInteraction → `update_action` 执行。
- `process_input_item_actions_schedule`：按装备（主手=槽 37/副手=所选快捷栏槽/护甲=槽 36）刷新 `InitialItemActions` 到动作槽；Shift/右键/T 触发；副手动作受主手动作 wield（Single 时屏蔽，Dual 放行）门控。
- `process_input_hotbar_in_hud`：1-6/滚轮选槽；F 主手↔选中槽整叠交换 + 发 OffHandSwapped。
- `process_input_toggle_inventory`：R 切换 HeadsUpDisplay/Inventory overlay。
- `player_scene(spawn_point)`（P1 ✅）：玩家实体场景（圆柱 r0.5 h1.7 绿色 + `player()` 组件集 + SpawnPoint + 初速 vy=10）；由 dim.rs 生成 observer 调用，玩家挂 RootNode 下（不进维度子树，传送/切维度存活）。

### dim/actor/living/ai.rs
AI 公共积木（设计：每怪一个 enum 状态机 + 专属系统）：`AiTarget(Option<Entity>)`、`AiPerception { sight_range=24, attack_range=2 }`、`AttackCooldown(1s Repeating)`（P2 起 zombie 使用）、`HitStun(0.25s Once)`、`LootTable(Vec<NamespacedKey>)`（无权重，Clone，**重复条目=多件掉落**）。系统：`perceive_targets`（视野内最近玩家，无视线遮挡，O(怪×玩家)）、`chase(controller, perception, my_translation, target_translation, speed)`（Tnua Walk 直线，desired_motion=方向×**speed**，P2 加 speed 参数——Tnua walk config 的 speed 默认 20 = 单位运动向量时的速度，不缩放则怪速 2× 于玩家；调用方传 `movement_speed.value()`；返回是否进攻击范围）、`tick_status`（全局 tick 所有 HitStun+AttackCooldown）。⚠️ 直线 chase 无寻路，怪可能卡死在障碍物上（P2 实测）。

### dim/actor/living/creeper.rs
`CreeperState { Idle, Chase, Fuse(f32) }`（Fuse 1.5s → Explosion power 4 → despawn）。`creeper()` 场景：living_actor + 圆柱碰撞体 + AiPerception{20, 1.5}。⚠️ 无 glTF 模型（裸圆柱）、无 LootTable（死亡无掉落）、Fuse 无膨胀/音效表现。系统 `creeper::system`：感知→追（chase speed=`movement_speed.value()`，P2）→引信；HitStun 期间冻结状态推进（不能取消引爆）。

### dim/actor/living/zombie.rs
P2 ✅ 近战怪（新文件）。`Zombie(ZombieState)`（Clone+Component，`#[require(AiTarget, AiPerception, AttackCooldown)]`）；`ZombieState { Idle, Chase, Attack }`（Attack 次帧回 Chase）。`ZombieWeapon { BareHand(default), Sword, Spear }` + `roll(rng: &mut impl RngExt)`（4/3/3：`random_range(0..10)`）。打击：剑=`MeleeStrike(6,6,120°,2)`、矛=`MeleeStrike(7,7,30°,2.5)`（均 `LazyLock<MeleeStrike>` 预计算）、空手=距目标 ≤1.5 直接 `Damage` 2-4（`random_range`）+径向击退 5（无扇形）。`zombie(weapon)` 场景：`living_actor(&KEY, false)` + `Collider::cylinder(0.5, 1.7)` + 绿色圆柱网格（srgb(0.15,0.8,0.1)）+ **`template_value(weapon)`**（枚举不能走 bsn patch 语法）+ AiPerception{24,2} + `LootTable(vec![EMBER_SHARD; 2])`。系统 `zombie::system`（查询排除 Player）：感知→追（speed=movement_speed 属性）→ Chase 且 in_range 且冷却就绪（let-chain）→ 按武器 strike + `cooldown.0.reset()` + 状态 Attack；HitStun 冻结状态推进。⚠️ 无 glTF 模型（绿色圆柱占位）。

### dim/actor/living/dummy.rs
测试假人：`dummy()` = living_actor + 1×3×1 碰撞体 + dummy.glb 模型 + DamageTaken 修正 `AddMultipliedValue(-1.0)` → **受伤恒为 0（无限血测试桩）**。

### dim/actor/living/attributes.rs
数据驱动属性系统：`AttributeType` trait + `BoxedAttributeType` 注册表（root=`attributes`）；`StandardAttributeType<A: TypeKey>`（按 actor key 解析 `AttributeBase` payload 取基础值；缺失→"虚拟"属性 base=NAN，取值 UB）。
`Attributes<A> { base, modifiers }`；`AttributeModifier { key, modification }`；`AttributeModification = AddValue(f32) | AddMultipliedValue(f32)`；`value()` = base + Σadd，再 × Π(1+mul)。
已注册 5 属性：`embers:damage_taken`、`embers:knockback_taken`、`embers:max_health`、`embers:melee_damage`（⚠️ 全库无消费方）、`embers:movement_speed`。`AttributesTemplate` 在生物生成时按 actor 注入全部属性。

### dim/actor/living/damage_number.rs
`FloatingText { world_position, timer 0.8s, drift }`。系统：`spawn_damage_numbers`（DamageNumber 消息 → DimensionViewNode 下红色 UI 文本，14px polygon 字体）、`update_damage_numbers`（上浮 0.8 单位 + camera.world_to_viewport 投影 + alpha 淡出 + 到期移除）。

### dim/actor/item_actor.rs
`ItemActor(Entity)`（引用物品实体）。`item_actor()` 场景：Phantom 物理（可交互层）+ 0.25³ cuboid + `default_scene()` 占位网格 + Interactable→pickup。`item_actor_of`/`item_actor_for`。⚠️ 无浮动/旋转动画、无存在时限/消失逻辑。

### dim/actor/primed_tnt.rs
`Fuse(Timer 4s)` + `fuse` 系统（到期 trigger Explosion{power:4} 并移除）。`primed_tnt()` 场景：MiscActor + 1³ 碰撞体 + tnt.glb。⚠️ 数值硬编码、无点燃表现。

### dim/actor/projectile.rs
💥 **只有一个 `projectile()` 场景构建器**（Projectile 物理预设）。无组件、无飞行/轨迹/命中系统、无生成器。投射物完全未实现。

### ui.rs
**两级状态机**：`GameState { MainMenu(默认), Dimension }` × `ActiveOverlay { GatewayMenu, HeadsUpDisplay, Inventory, LoadingScreen(默认), OptionsAudio, OptionsControls, OptionsLanguage, OptionsMain, OptionsVideo, PauseScreen, TitleScreen }`。
- `process_escaping`（PreUpdate）：Escape 回退栈——HUD→Pause；Gateway/Inventory/Pause→HUD；4 个选项子页→OptionsMain；OptionsMain→HUD(维度内)/TitleScreen(菜单)；Loading/Title 不响应。
- `process_directional_navigation`：方向键 AutoDirectionalNavigator。
- `NodeInteraction` 实体事件 + `trigger_default_node_interaction`（鼠标点击 或 焦点下 Enter → 触发）；按钮点击音效被注释（⚠️ 无音频）。
- `text()`（polygon 字体）、`text_button`（200×20 按钮 + hover/focus/disabled 三态贴图切换 observer）。
- `TextureScaling`（Auto/Stretch/Sliced/Tiled，.scaling.toml）、`TextureAnimation`（atlas 帧动画，.animation.toml）、`AnimatedTexture` + `run_animations`。
- `SetWindowIcon`（icon 加载后显示窗口+设 icon）；`UiScale(3.)`；`RootNode` 组件。
- ⚠️ 插件列表注册了 dim/hud/inventory/gateway_menu/loading_screen/main_menu/options_main/pause_screen/title_screen——**没有 options_video**（options_audio/controls/language 无文件）。

### ui/dim.rs
`OnEnter(GameState::Dimension)`：生成 `DimensionNode`（RootNode+全屏 flex）→ `DimensionViewNode`（3D 视口容器，HUD/Pause/Inventory/GatewayMenu 挂它下）+ 正交 3D 相机（16:9 固定、Bloom）。`resize_camera`（窗口变化按 16:9 信箱式调 viewport）；`update_player_camera`（等距跟随：距离 12、高度 8、俯角 35°，look_at 玩家，`Single<&Transform, With<Player>>` 假定唯一玩家）。`PlayerCamera` 仅 `Isometric` 一个变体（⚠️ 无第一/第三人称）。

### ui/title_screen.rs
标题图（debug: title_dev / release: title）+ 三按钮：Play（`Load::EnterDimension(EnterWorld, embers::LOBBY)`，⚠️ 硬编码 lobby，无世界选择）、Options、Quit。TabGroup 键盘导航 + 版本号。

### ui/main_menu.rs
`OnEnter(GameState::MainMenu)` 生成全屏 RootNode 骨架（Camera2d + flex 居中），无按钮（内容由 TitleScreen overlay 提供）。

### ui/loading_screen.rs
**加载流程核心**：`Load` 事件（EnterDimension(context, key) / EnterMainMenu(context) / Reload）；`DimensionEntryContext { EnterWorld, GatewayTravel, PortalTravel }`（GatewayTravel/PortalTravel 任务图 P1 ✅，原 todo!() 已替换）；`MainMenuEntryContext { Init, ExitWorld, SaveAndExitWorld }`。
**加载任务 DAG**（⚠️ 作者自注 `TODO: Fix / Faulty logic ahead`）：任务=实体组件（GameStateTransitionTask/DimensionGenerationTask/WorldSavingTask/Fetch/Evict Scope/Mount/Unmount Source/Refetch/ReloadMetadata），`TaskDependencies` 实体关系表达依赖；`begin_loading` 按 Load 变体 spawn 任务图 → `init_tasks` 启动叶子任务 → 完成逐级解锁；全部完成回写 LoadingOverlay 到 NextState。
`WorldSavingTask` ⚠️ = 空闭包（**保存并退出实际不保存**）。
`LoadingScreenSettings`（load_tip/target_tip/background:Option<()>⚠️ 未实现）；UI：左下提示 + 右下 loading 动画图。
- EnterWorld 任务链：DimensionGeneration ← GameStateTransition(Dimension) ← ReloadMetadata ← FetchScope(Dimension)。
- GatewayTravel/PortalTravel 任务链（P1 ✅）：DimensionGeneration ← ReloadMetadata ← FetchScope(Dimension)（无状态迁移，已在 Dimension 态）；loading 提示 "Preparing warp"/"Traveling to"。
- Init 链：ReloadMetadata ← [MountSource(embers), FetchScope(Global)]。
- Exit 链：GameStateTransition(MainMenu) ← ReloadMetadata ← EvictScope(当前维度)；SaveAndExit 并行加 WorldSavingTask。

### ui/heads_up_display.rs
HUD（最完整的 overlay）：进入时锁定光标（Confined）。6 格快捷栏（16×16 物品图标 + hud/hotbar 背景 + 可移动高亮指示框）+ 主手槽（hud/main_hand 背景）+ 血条（底+红填充）。系统：`update_health_bar`（宽度比例）、`update_hotbar_selection_indicator`（SelectedHotbarSlot 驱动）、`update_hotbar`（Changed<Inventory> 时刷新图标）。OnExit 释放光标。

### ui/inventory.rs
⚠️ 占位：仅 "Inventory" 文本，无格子/拖拽/交互。`fina()` 空。

### ui/pause_screen.rs
⚠️ 占位：仅 "Paused" 文本，无按钮（只能 Escape 返回）。

### ui/gateway_menu.rs
⚠️ 占位：仅 "Gateway" 文本。**P1 后无入口**（原 gateway_travel 交互已改为直接触发 Load，全库无代码再设置 `ActiveOverlay::GatewayMenu`，仅 ui.rs 回退栈与自身 OnEnter 还引用该状态）。

### ui/options_main.rs
Grid：标题 + Audio/Controls/Language/Video 四按钮（切对应 overlay，⚠️ 全是死状态）+ Done（模拟 Escape 返回）。

### ui/options_video.rs
⚠️ **空文件**，plugin 未注册。

## embers-macros/src/lib.rs
- `#[identify(字段)]`（属性宏）：struct 基于指定字段实现 PartialEq/Eq/Hash。
- `#[derive(TypeKey)]`：需 `#[type_key = "ns:key"]` 属性，实现 `utils::TypeKey` trait（`key()` 返回 LazyLock 缓存的 &'static NamespacedKey）。

## bsn! 场景宏语法坑（P1 实测验证，bevy_scene_macros 0.19）
- `#Ident` 是**实体名**（插入 Name 组件），不是组件；逗号分隔不同实体，换行/空白分隔同一实体的多个组件。
- `Component(expr)` 的参数按 BsnValue 解析，**不支持方法调用**（如 `.clone()`）；复杂表达式须包 `{}`（宽松表达式块），如 `Dimension({embers::LOBBY.clone()})`、`initial_click: { Some(key) }`。
- `Component(value)` 是 **patch 语义**（先 `Default::default()` 再逐字段覆盖）→ 值类型须 **Clone + Default**（P2 实测：LootTable/PortalTimer 因此加了 Clone/Default derive）；**枚举值不能用 patch 语法**（枚举没有字段 0，E0609 "no field 0"）→ 用 `template_value(value)`（bevy 0.19 中每个 Component 自动是 Template）。
- 嵌套场景函数用 `( fn_name() )` 展开合并到当前实体；`Children [ ... ]` 列表。
- 结构体字段构造（`Comp { field: value }`）：未提及字段取 Default（该组件须 `#[derive(Default)]`，字段值需 Clone）；**不支持 `..default()` 结构更新语法**（编译期语法错）。
- 场景结构持有值直到命令 flush：引用值（如迭代器里的 `&Vec3`）必须先拷贝为 owned 局部变量再入场景，否则借用生命周期不够（E0597）。
- `asset_value(...)` 来自 bevy_asset::handle（动态资产句柄模板）；`template_value(...)` 为值模板。

## bevy 0.19 API 坑（P2 实测验证）
- **事件系统 = 纯 observer**：无 `Events<T>` 通道/`Events` 系统参数；触发用 `World::trigger(event)` 或系统参数 `Commands::trigger(event)`（立即执行匹配 observer），消费用 `add_observer(|e: On<E>| ...)`。项目里 `Load`/`Explosion`/`DimensionGenerationRequest` 均如此。`Damage`/`DamageNumber`/`OffHandSwapped` 是 **Message**（`write_message`/`read_message`），与 Event 是两套机制。
- Query 方法名：`count()`（不是 `len()`）、`single()`（不是 `get_single()`）；`iter`/`iter_mut`/`get`/`get_mut`/`par_iter` 保留。
- `FrameCount(pub u32)` 在 **`bevy::diagnostic`**（bevy_core 已不存在），DefaultPlugins 含 `FrameCountPlugin`，Last 阶段自增（首帧 Update 里是 0）。
- `Timer::from_seconds(f32, TimerMode)`（第二参数是 `TimerMode` 不是 bool）；`is_finished()`（不是 `finished()`）；`just_finished()` 在 Once 模式只有一帧为 true（做一次性 spawn 用）。
- **`&mut T`（T: FromWorld）不再是合法系统参数** → 须 `init_resource::<T>()` + `ResMut<T>`。
- Resource 必须显式 `#[derive(Resource)]`（不再自动实现）。
- 元组结构体字段默认私有：跨模块读 `pub struct Foo(T)` 的 `.0` 会 E0616（本项目 `Dimension`/`PortalTimer` 字段私有）。
- rand = **0.10**：无 `rand::Rand` 类型；`random_range` 在 **`RngExt`** trait（不在 `Rng` 上）；`rand::rngs::SmallRng: SeedableRng`。
- bevy-tnua 0.32：`TnuaBuiltinWalk` 的 `desired_forward` 会驱动身体转向（角色朝向=本地 **-Z**）；config `speed` 默认 **20.0** = 单位 desired_motion 下的速度（玩家 10 = 20×movement_speed 0.5）。

## pld/ 数据文件（embers 命名空间）
- `global/actors/embers/`：player.actor.toml（float_height=1, max_health=20, movement_speed=0.5）、creeper.actor.toml（float_height=1, max_health=20, movement_speed=0.5）、zombie.actor.toml（P2：float_height=1, max_health=30, movement_speed=0.5）、tnt.actor.toml/item.actor.toml（**空文件**，用默认）。
- `global/items/embers/`：sword（weight=1, hands_click=sword_attack_0）、spear（weight=2, hands_click=spear_attack_0, hands_double_click=spear_throw）、tnt（max_stack_size=16, weight=0.2, hands_click=tnt_throw）、ember_shard（P2：weight=0.05, max_stack_size=64，P4 合成材料）。
- `global/item_actions/embers/`：sword_attack_0（melee：dmg7 kb7 扇120° 距5 Single 0.5s 链自身）、spear_attack_0（melee：dmg8 kb8 扇30° 距6 Dual 0.5s 链自身）、spear_throw（charged_throw：Dual，**缺 hold_threshold/hold_action，蓄力后无行为**）、tnt_throw（throw：速度20 Single 0.25s）。
- `global/blocks/embers/dark_oak_planks.block.toml`：collider=embers:full，[model] 注释；⚠️ 管线停用不会加载。
- `global/fonts/embers/polygon.ttf`（UI 字体，键 `embers:polygon`）。
- `global/textures/`：blocks（bricks、dark_oak_planks）、items（spear/sword/tnt）、particles（explosion_0-15、generic_0-7、damage_indicator）、ui/widgets（button 三态 + scaling）、ui（loading_indicator.png+atlas+animation、title.png、title_dev.png）、ui/hud（hotbar.png 122×22、main_hand.png 22×22、hotbar_selection.png 24×23，**全透明占位**）。
- `global/models/`：tnt.glb、missingno.glb（占位模型）。
- `dim/embers/lobby/`：actors/embers/dummy.actor.toml + models/actors/embers/dummy.glb（dummy 模型在 lobby 维度作用域）。

## 已知缺口清单（改代码前先对照）
1. 💥 todo!()：chunk.rs（体素网格，`GreedyChunkMesher::generate_mesh` 等）。
2. 💥 unimplemented!()：inv.rs L450（区间→区间 move_item）。
3. ⚠️ WorldSavingTask 空闭包 → 无存档系统。
4. ⚠️ 体素世界整体未实现（block/chunk 注释停用；bevy_voxel_world 死依赖）；无方块破坏/放置。
5. ⚠️ projectile.rs 完全未实现；charged_throw 只 println。
6. ⚠️ 选项子页全部死状态（video 空文件未注册，audio/controls/language 无文件）。
7. ⚠️ MeleeDamage 属性无消费方；Enchantments/RangedAmmo 空组件；护甲槽无效果。
8. ⚠️ Creeper/Zombie 均为圆柱占位模型（Zombie 有 LootTable=2×ember_shard，Creeper 无掉落）；玩家是圆柱占位；玩家死亡无死亡 UI/流程（P3 已加背包清空+回 lobby 惩罚，但仍无确认界面）；地面物品无动画/无消失时限；怪物直线 chase 无寻路（可能卡死在障碍物上）。
9. ⚠️ 无音频系统（sounds payload 根已声明但无文件）；无改键 UI；无世界选择（Play 硬编码 lobby）；加载逻辑自述有缺陷（L44 TODO: Fix）。
10. ⚠️ 爆炸不破坏方块、不连锁 TNT（dim.rs L782 TODO）。

## 当前主线任务（2026-08 与用户确认中）
目标：搜打撤核心循环 —— lobby 合成装备 → 进地图战斗/拾取物资 → 限时后出现传送门 → 传送回 lobby。UI 缺陷/设置页暂缓。计划文件：`.trae/plans/extraction-core-loop.md`。
- P1 维度传送机制 ✅（用户已实测验收）：维度场景按 key 分发（lobby/operation）、双向 gateway 直接触发 Load、GatewayTravel 任务图、玩家/背包与维度解耦。已修两个 bug：① gateway 掉穿地面（改 Static 刚体 + 显式 collider）；② 卡在 "Preparing warp"（manager 轮询加载状态，处理"目录不存在"/"重复加载已加载目录"不发事件的情况）。
- P2 operation 战斗地图 ✅（2026-08-24 完成并运行时自动验证）：40×40 地面 + 7 cuboid 障碍；场景生成时随机刷 creeper×3 + zombie×5（避开入口半径 6）+ 散落 10 物资（8×ember_shard + 2×随机武器）；PortalTimer(90s) 到期生成回 lobby 的传送门（常驻）；zombie=新近战怪（生成时随机武器 4/3/3 剑/矛/空手，复用 MeleeStrike）；近战共享化 `MeleeStrike` + **扇形朝向 +90°Y 修正（玩家攻击方向也变了，用户实测需确认）**；ember_shard.item.toml 提前创建（P4 用）。运行时验证（env EMBERS_AUTO_TEST 门控临时代码，验证后已清除，main.rs 归零）：刷怪数 3/5/10 正确、僵尸近战命中玩家（-7=矛伤）、传送门恰在进图 90s 出现（interactables 10→11 常驻）、0 模板错误。踩坑：**zombie.actor.toml 未手动复制到 shp → 场景构建失败**（见"pld→shp 手动镜像"）。fmt/clippy/test(34) 通过，待用户手动实测。
- P3 死亡惩罚 ✅（用户已实测验收）：operation 死亡=清空背包+PortalTravel 回 lobby 满血重生；lobby 死亡/加载态=瞬移 SpawnPoint 兜底（实现见 living.rs `damage` 死亡分支）。
- 空气墙穿墙 bug ✅（2026-08-25 定位修复）：用户报"走到角落直接出去/击退穿墙"。曾两次错误修复（加厚墙 2.0、LivingActor 加 SweptCcd）均无效，已 `git revert` 回滚。真根因=**`Collider::cuboid` 收全尺寸，原代码传了半尺寸**→墙碰撞体只有视觉一半长，40m 地图角落无碰撞体；修复=重写 `dimension_barrier`（全尺寸、厚 1.0、±Z 墙加长盖角）+ `obstacle()` 同款修复。运行时验证（env 门控临时 autotest，已清除，main.rs 归零）：4 墙 collider (42,5,1)/(1,5,40) 位置/layers(32,29)/static tree 注册全部正确；同层碰撞对实测通过（LivingActor 球落在地面滚动）。待用户实测：走到角落、击退/爆炸是否还穿墙。
- P4 待完成：合成系统（配方 8×shard→sword / 12×shard→spear / 4×shard→tnt、合成站实体、Crafting overlay）。
- P5 待完成：初始物资（8×ember_shard 进图携带）+ 数值调参 + 端到端验证。
