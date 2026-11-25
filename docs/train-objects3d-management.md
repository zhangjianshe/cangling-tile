# 3D Tiles 缓存管理部署与验收（2026-11-25）

入口：https://train.cangling.cn/cangling-tile/ → 管理员登录 → 3D Tiles 缓存。

## 部署

主机 `ssh -p2225 root@changping.cangling.cn`，K3s cangling/cangling-tile。
继续使用原镜像和环境/挂载，command改为
`/app/tiledata/.deploy/20261125-management/cangling-tile`。
对应主机路径 `/opt/cl/local/cangling/tiledata/.deploy/20261125-management/cangling-tile`。
本次使用musl静态构建，SHA-256：
`7511c2ead336b5846c19f08e4a3a0b3e636ef0f7ce618b01468d967525d513e1`。
部署前配置存于同目录 `deployment-before.json`。回退command至
`/app/tiledata/.deploy/20261125-cache/cangling-tile`，不需数据库迁移。
后续用正式镜像部署时须移除command覆盖。

## 验证

- `cargo test --locked`：31项通过，含新增管理员会话、匿名/Bearer越权拒绝、跨站删除拒绝、
  指定版本清理计数、保留另一个版本、退出后拒绝访问的断言。
- `node --check src/objects3d.js`通过。
- `node scripts/test-objects3d-ui.cjs`：真实前端脚本在模拟DOM中验证登录状态、分页、查询、取消、
  清理、刷新、错误处理、会话过期及迟到响应隔离；这不替代浏览器视觉检查。
- 公网JS与本机构建源一致。未登录GET管理列表返回401。
- 使用现有管理员凭证经公网正常登录，列表显示Dragon4文件/4,791,036字节/9次命中，
  郑州31文件/1,864,472字节/33次命中。
- 经管理员接口清理Dragon指定版本，实际删除4文件/4,791,036字节，列表不再显示该版本；
  郑州数量、大小和命中次数不变。
- 经Map重新读取Dragon4文件，全部MISS回填、再次全部HIT，内容哈希与发布源一致。
- 最终恢复35文件；主动退出测试登录后管理接口返回401。具体结果见同目录JSON。
- 最终Pod 1/1 Ready，容器重启0次。
- 本轮Chrome和内置浏览器控制连接持续超时，未能完成可视化布局及真实浏览器点击验证；
  已完成公网HTTP端到端验收及前端组件状态测试，不能把二者表述为浏览器验收。
