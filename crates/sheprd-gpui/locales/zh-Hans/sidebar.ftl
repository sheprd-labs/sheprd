sidebar-header-spaces = 工作区
sidebar-header-devices = 设备
sidebar-header-agents = 智能体
sidebar-label = 侧边栏
sidebar-workspaces-label = 工作区
sidebar-devices-label = 设备
sidebar-agents-label = 智能体
sidebar-collapse-all-hosts = 折叠所有主机
sidebar-expand-all-hosts = 展开所有主机
sidebar-no-agents = 暂无智能体
sidebar-no-matching-agents = 没有匹配的智能体

sidebar-new = 新建
sidebar-new-workspace = 新建工作区
sidebar-menu = 菜单
sidebar-menu-label = 菜单

sidebar-sort-grouped = 按工作区分组
sidebar-sort-priority = 按优先级排序
sidebar-agents-sort = 智能体排序：{ $sort }
sidebar-agents-view = 智能体：{ $view }

sidebar-status-working = 工作中
sidebar-status-blocked = 等待处理
sidebar-status-done = 已完成
sidebar-status-idle = 空闲
sidebar-status-unknown = 未知

sidebar-spoken-separator = { "，" }
sidebar-spoken-linked-worktree = 关联工作树
sidebar-spoken-connected = 已连接
sidebar-spoken-disconnected = 已断开
sidebar-spoken-working = { $count } 个正在工作

sidebar-rail-in = { $place }中的{ $name }
sidebar-rail-status = { $text }（{ $status }）
sidebar-rail-host = { $host }：{ $text }
sidebar-rail-collapsed = { $host }（已折叠）

sidebar-origin-local = 本机
sidebar-origin-ssh = SSH 主机
sidebar-origin-tailscale = 通过 SSH 连接的 Tailscale 主机
sidebar-origin-wsl = WSL 发行版
sidebar-origin-cloud = { $provider } { $noun ->
        [VM] 虚拟机
        [container] 容器
        [sandbox] 沙盒
        [sprite] Sprite
        [workspace] 工作区
       *[machine] 云主机
    }
sidebar-origin-cloud-ssh = 通过 SSH 连接的{ $provider } { $noun ->
        [VM] 虚拟机
        [container] 容器
        [sandbox] 沙盒
        [sprite] Sprite
        [workspace] 工作区
       *[machine] 云主机
    }
sidebar-workspace-move-not-sent = 未能发送工作区移动请求：{ $error }

sidebar-primary = 主工作树
sidebar-spoken-muted = 通知已静音
sidebar-pull-request-unopened = +PR

sidebar-group-all = 所有分组
sidebar-agents-group = 智能体分组：{ $group }
sidebar-coordinator-row = 协调器：{ $row }
