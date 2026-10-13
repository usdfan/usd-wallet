# LightSNS USD Wallet 1.0.2

本项目保留 LightSNS 原生钱包、余额记账和提现审核流程，只改变前台金额单位的显示，以及提现表单的收款信息。**显示为 USD 不会改变后端计价货币、支付网关结算货币或汇率。**

## 源码目录

- `shared/`：PC 和移动端共用的 Rust/WASM 接口钩子、前端脚本、样式和 WIT 定义。
- `pc/`、`mobile/`：两个 LightSNS 模块的清单、构建配置及已编译组件。
- `build.py`：生成两个 `.lsmod` 安装包的打包脚本。
- `dist/`：当前 1.0.2 版本的 PC 和移动端安装包。

## 安装包

LightSNS 的 `gadget` 清单一次只能声明 `pc` 或 `mobile`，所以同一个项目提供两个必须同时安装的官方模块包：

- `dist/usd-wallet-pc-1.0.2.lsmod`
- `dist/usd-wallet-mb-1.0.2.lsmod`

两个包只在各自的全局 body-end 插槽加载脚本；都登记 PC 和移动端提现接口的服务端钩子。只安装其中一个也会保留服务端拦截，但只覆盖对应端的界面。

在 LightSNS 后台依次上传、启用两个 `.lsmod`，然后重启 LightSNS 服务，使 WASM 注册和前端资源生效。不要把包解压覆盖到官方 `pc/`、`mb/`、`static/` 或 Rust 二进制目录。

## 上线前配置

1. 在原生「财务 → 货币管理 → 提现货币设置」中，为允许提现的货币启用 `bank` 渠道。模块仅把这个现有渠道显示成 USDT；若原生没有启用 `bank`，模块不会虚构可用渠道。
2. 按实际 USDT 人工打款规则核对提现比例、手续费、最低额与日限额。原生提现计算的 `actual_amount` 仍沿用原来的人民币/货币比例逻辑。若要把数字原样当作 USDT 打款，需由站长确认相应提现比例为 1:1，且资金来源、结算币种与记账含义一致。
3. 核对充值订单和第三方支付网关的真实结算币种。模块会把用户看到的 `¥`、`￥`、`人民币`、`RMB` 改成 `$` / `USD`，**不会把 CNY 转换成美元**。结算币种仍为 CNY 的页面不得因为显示 `$` 就被当作美元订单上线。

## 数据映射与保护

| 表单 | 原生提现字段 |
| --- | --- |
| 提现方式 USDT | `channel=bank` |
| 钱包地址 | `account_no` |
| TRC20 / ERC20 / Polygon | `account_extra` |
| 原生必填账户标识 | `account_name=USDT` |

入口钩子挂在 `/api/pc/wallet/withdraw` 和 `/api/mb/wallet/withdraw` 上。对非 `bank` 渠道、未知网络、无效地址、重复字段或不可读的请求正文返回拒绝，不执行原生提现处理。TRC20 使用 Base58Check 校验，EVM 地址检查格式及混合大小写的 EIP-55 校验。余额、限额、手续费、冻结、提现记录、管理员审核和拒绝退款由原生接口继续处理。

出口钩子只把钱包初始化与提现成功回包中的渠道过滤为 `bank` 并把显示名改成 `USDT`；移动端按货币配置的 `withdraw.configs[*].channels` 同样过滤。提现表单的收款信息区域只展示「选择提现网络」和「USDT 钱包地址」，不展示收款人或开户行；原生必填的账户标识及网络映射字段由模块自动填入，并沿用原生接口提交。

## 本地验证状态

- PC、移动端 WASI P2 组件已编译；`.lsmod` 信封、SHA-256、zstd tar 成员和组件头已检查。
- Rust 单元测试覆盖渠道拒绝、地址校验与移动端按货币渠道过滤；前端 JavaScript 通过语法检查。
- **尚未在运行中的 LightSNS 上完成真实提现申请、冻结、管理员审核、拒绝退款和人工打款验证。**用户要求暂缓站点测试，因此 `bank` 对钱包地址的实际接收行为尚未得到交易级确认。上线前必须用测试账户完成一笔申请和拒绝退款，再做人工打款验收。

## 重新构建

安装 Rust 1.85+、`wasm32-wasip2` 目标，以及 Python `zstandard` 或 Node.js 24+，分别构建 `pc/Cargo.toml`、`mobile/Cargo.toml` 的 release WASM，并让两者共用 `usd-wallet/target` 目录。然后运行 `python build.py`。仅修改前端时可复用各模块 `backend/module.wasm`，直接运行打包脚本。脚本将共用前端资源与两个 WASM 组件复制到各自模块目录，生成并校验两个 `.lsmod`。

参考：[LightSNS 模块开发](https://lightsns.com/docs/module)、[页面注入槽](https://lightsns.com/docs/module/slots)、[接口钩子](https://lightsns.com/docs/module/api-hooks)、[钱包接口](https://lightsns.com/docs/api/payment)。
