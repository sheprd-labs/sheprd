overview-title = 概览
overview-stat-needs = { $count } 项待处理
overview-stat-working = { $count } 项工作中
overview-stat-ready = { $count } 项待审查
overview-pinned = 已置顶
overview-needs-you = 需要处理
overview-ready-for-review = 待审查
overview-working = 工作中
overview-idle = 空闲
overview-snoozed = 已暂缓
overview-settled = 已处理
overview-closed = 已关闭
overview-alerts = 最近提醒
overview-long-idle = 另有 { $count } 项空闲
overview-row-detail = { $workspace } · { $detail }
overview-wakes = { $time }恢复提醒
overview-woke = 已恢复提醒
overview-run-failed = 运行失败：{ $title }

overview-reopen = 重新打开
overview-forget = 移除记录
overview-snooze = 暂缓提醒
overview-settle = 标为已处理
overview-pin = 置顶
overview-unpin = 取消置顶
overview-unsettle = 标为待处理
overview-wake-now = 立即恢复提醒
overview-open = 打开
overview-reply = 回复
overview-snooze-until = 暂缓至
overview-preset-hour = 1 小时后
overview-preset-hours = 3 小时后
overview-preset-evening = 今天晚上
overview-preset-tomorrow = 明天
overview-preset-next-week = 下周
overview-weekday-time = { $day ->
        [mon] 周一
        [tue] 周二
        [wed] 周三
        [thu] 周四
        [fri] 周五
        [sat] 周六
       *[sun] 周日
    } { $time }
overview-elapsed-now = 刚刚
overview-elapsed-minutes = { $minutes } 分钟
overview-elapsed-hours = { $hours } 小时
overview-elapsed-days = { $days } 天

overview-empty-title = 暂无智能体
overview-empty = 此处显示此窗口连接的各台主机上的智能体，按待处理或失败、待审查、工作中和空闲的顺序排列。可直接回复、暂缓提醒或标为已处理，也可在“已关闭”中重新打开已关闭的智能体。

overview-hosts = 主机
overview-connecting = 正在连接…
overview-not-connected = 未连接
overview-agents = { $total ->
        [0] 暂无智能体
       *[other] { $total } 个智能体
    }
overview-agents-working = { $total } 个智能体 · { $working } 个工作中
overview-meter-cpu = CPU
overview-meter-memory = 内存
overview-meter-gpu = GPU
overview-meter-vram = 显存

overview-session-gone = 智能体的原会话已断开
overview-tab-opening = 另一个标签页正在打开
overview-host-switch = 无法切换到智能体所在的主机
overview-host-timeout = 智能体所在的主机响应超时
overview-workspace-closed = 智能体所在的工作区已关闭
overview-reopen-failed = 无法重新打开智能体：{ $error }

overview-selected = 已选择 { $count } 项
overview-selection-clear = 清除选择

overview-reply-title = { $count ->
        [1] 回复{ $name }
       *[other] 回复 { $count } 个智能体
    }
overview-reply-note = 回复会在智能体回到提示符时发送。等待批准期间，回复会留在此窗口中排队。
overview-reply-field = 回复
overview-reply-send = 将回复加入队列
overview-reply-cancel-dialog = 取消
overview-reply-too-long = 回复过长。请缩短内容，或打开智能体后粘贴完整回复。
overview-reply-waiting = 正在等待提示符
overview-reply-sending = 正在发送
overview-reply-failed = 发送失败：{ $error }
overview-reply-cancel = 取消
overview-reply-dropped = 智能体已离开，回复未发送：{ $text }
overview-reply-unsupported = 回复需要在智能体的主机上运行 herdr 命令，此客户端无法运行该命令
overview-reply-skipped = 有 { $count } 个所选项目无法接收回复
overview-reply-not-queued = 有 { $count } 条回复未加入队列

overview-alert-gone = 对应的窗格已关闭
overview-alerts-empty = 暂无提醒
overview-alerts-clear = 清除
overview-alerts-clear-hint = 清除所有窗口的提醒、弹出提示以及 Sheprd 的系统通知
overview-core-host = 本机
