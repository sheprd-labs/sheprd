(function () {
  "use strict";

  // Global Platform Switcher (Desktop + iOS / Desktop / iOS)
  var viewBtns = document.querySelectorAll("#platform-view button");
  viewBtns.forEach(function (btn) {
    btn.addEventListener("click", function () {
      var mode = btn.getAttribute("data-view");
      viewBtns.forEach(function (b) { b.classList.toggle("active", b === btn); });
      document.body.classList.remove("view-both", "view-desktop", "view-ios");
      document.body.classList.add("view-" + mode);
    });
  });

  // Sync iOS #1 Escalation preview when user switches desktop escalation step
  var iosEsc = document.getElementById("ios-esc-screen");
  var escStepBtns = document.querySelectorAll("#esc-steps button");
  function renderIosEsc(step) {
    if (!iosEsc) return;
    if (step === 0) {
      iosEsc.className = "iphone-screen";
      iosEsc.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill">✳ Claude</div>' +
        '<div class="ios-kicker" style="text-align:center;margin-bottom:6px">IN CHAT ON IPHONE</div>' +
        '<div class="ios-glass amber">' +
        '  <div class="ios-kicker" style="color:#f6c978">INLINE PINNED BAR</div>' +
        '  <div class="ios-title">🔑 Tap YubiKey at MacBook Pro</div>' +
        '  <div class="ios-copy">Shown inline inside chat — no full-screen sheet while you read it.</div>' +
        '</div>' +
        '<div class="ios-bg-list" style="margin-top:auto"><div class="ios-bg-row"><span>Rebased 4 commits onto main…</span></div></div>';
    } else if (step === 1) {
      iosEsc.className = "iphone-screen";
      iosEsc.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill">●</div>' +
        '<div class="ios-glass amber" style="margin-bottom:8px">' +
        '  <div class="ios-kicker" style="color:#f6c978">AGENTBANNERWINDOW (.ALERT)</div>' +
        '  <div class="ios-title">Tap YubiKey at MacBook Pro</div>' +
        '  <div class="ios-copy">Persistent Liquid Glass banner above Workspaces &amp; sheets.</div>' +
        '  <div class="ios-btn-row"><button class="ios-btn amber" type="button">Open chat</button><button class="ios-btn subtle" type="button">Dismiss</button></div>' +
        '</div>' +
        '<div class="ios-bg-list"><div class="ios-bg-row"><span>herdr-gpui</span><span>2 chats</span></div><div class="ios-bg-row"><span>billing-api</span><span>idle</span></div></div>';
    } else {
      iosEsc.className = "iphone-screen lockscreen";
      iosEsc.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill amber">🔑 Tap YubiKey · 0:22</div>' +
        '<div class="ios-lock-clock"><div class="date">Thursday, Nov 26</div><div class="time">10:52</div></div>' +
        '<div class="ios-glass amber">' +
        '  <div class="ios-kicker" style="color:#f6c978">LIVE ACTIVITY · TIME SENSITIVE</div>' +
        '  <div class="ios-title">Tap YubiKey at MacBook Pro</div>' +
        '  <div class="ios-copy">Claude · <code>git push</code> blocked 22s. Physical key needed at Mac.</div>' +
        '  <div class="ios-btn-row"><button class="ios-btn subtle" type="button">Snooze 2m</button><button class="ios-btn amber" type="button">Open</button></div>' +
        '</div>' +
        '<div class="ios-haptic-pill">📳 Haptic double-knock · App Lock safe</div>';
    }
  }
  escStepBtns.forEach(function (b) {
    b.addEventListener("click", function () { renderIosEsc(Number(b.getAttribute("data-step"))); });
  });
  renderIosEsc(2);

  // Sync iOS #2 Typed Modals preview with preset switcher
  var iosTyped = document.getElementById("ios-typed-screen");
  var presetBtns = document.querySelectorAll("#preset-tabs button");
  function renderIosTyped(kind) {
    if (!iosTyped) return;
    if (kind === "choices") {
      iosTyped.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill">✳ Claude</div>' +
        '<div class="ios-bg-list"><div class="ios-bg-row"><span>herdr-gpui · PR #1428</span></div></div>' +
        '<div class="ios-sheet">' +
        '  <div class="ios-grabber"></div>' +
        '  <div class="ios-kicker">PERSISTENT SHEET · CLAUDE</div>' +
        '  <div class="ios-title">Handle 3 failing snapshots?</div>' +
        '  <div class="ios-copy">Tap a choice to reply straight from iPhone.</div>' +
        '  <div style="display:flex;flex-direction:column;gap:5px">' +
        '    <button class="ios-btn primary" type="button" data-ios-fb="Sent: Update snapshots">Update snapshots</button>' +
        '    <button class="ios-btn subtle" type="button" data-ios-fb="Sent: Revert padding">Revert padding</button>' +
        '    <button class="ios-btn subtle" type="button" data-ios-fb="Opened Changes tab">View diff</button>' +
        '  </div>' +
        '  <div class="ios-fb-slot"></div>' +
        '</div>';
    } else if (kind === "countdown") {
      iosTyped.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill amber">⏱ Deploy 15s</div>' +
        '<div class="ios-lock-clock"><div class="date">Lock Screen Live Activity</div><div class="time" style="font-size:26px">10:52</div></div>' +
        '<div class="ios-glass amber">' +
        '  <div class="ios-kicker" style="color:#f6c978">COUNTDOWN · APPINTENT</div>' +
        '  <div class="ios-title">Deploying staging-eu-1 in 15s</div>' +
        '  <div class="di-progress" style="margin:6px 0 9px"><i style="width:72%;background:var(--amber)"></i></div>' +
        '  <div class="ios-btn-row">' +
        '    <button class="ios-btn danger" type="button" data-ios-fb="Cancelled from Lock Screen">Cancel</button>' +
        '    <button class="ios-btn primary" type="button" data-ios-fb="Triggered immediately">Run now</button>' +
        '  </div>' +
        '  <div class="ios-fb-slot"></div>' +
        '</div>';
    } else if (kind === "otp") {
      iosTyped.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill amber">🔐 2FA Code</div>' +
        '<div class="ios-sheet">' +
        '  <div class="ios-grabber"></div>' +
        '  <div class="ios-kicker">ONE-TIME CODE · REDACTED</div>' +
        '  <div class="ios-title">Enter 6-digit npm OTP</div>' +
        '  <div class="ios-copy">Uses iOS <code>.oneTimeCode</code> AutoFill from Passwords / Messages.</div>' +
        '  <div style="padding:7px;border-radius:9px;background:#10131b;border:1px solid var(--amber);text-align:center;font:700 14px/1 var(--hk-mono);letter-spacing:4px;margin-bottom:8px">482 910</div>' +
        '  <button class="ios-btn amber" style="width:100%" type="button" data-ios-fb="Sent OTP over E2E tunnel">AutoFill &amp; Send</button>' +
        '  <div class="ios-fb-slot"></div>' +
        '</div>';
    } else {
      iosTyped.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill amber">↗ GitHub SSO</div>' +
        '<div class="ios-sheet">' +
        '  <div class="ios-grabber"></div>' +
        '  <div class="ios-kicker">OAUTH HANDOFF</div>' +
        '  <div class="ios-title">Approve GitHub Device Login</div>' +
        '  <div class="ios-copy">Code <code>A4F9-2C81</code> ready. Opens Safari &amp; auto-clears on callback.</div>' +
        '  <button class="ios-btn primary" style="width:100%" type="button" data-ios-fb="Opened Safari SSO · auto-cleared">Open github.com ↗</button>' +
        '  <div class="ios-fb-slot"></div>' +
        '</div>';
    }
    iosTyped.querySelectorAll("[data-ios-fb]").forEach(function (b) {
      b.addEventListener("click", function () {
        var slot = iosTyped.querySelector(".ios-fb-slot");
        if (slot) slot.innerHTML = '<div class="ios-feedback">✓ ' + b.getAttribute("data-ios-fb") + '</div>';
      });
    });
  }
  presetBtns.forEach(function (b) {
    b.addEventListener("click", function () { renderIosTyped(b.getAttribute("data-preset")); });
  });
  renderIosTyped("choices");

  // iOS #3 Secret-Safe Vault button
  var iosVaultBtn = document.getElementById("ios-vault-send");
  var iosVaultFb = document.getElementById("ios-vault-fb");
  if (iosVaultBtn && iosVaultFb) {
    iosVaultBtn.addEventListener("click", function () {
      iosVaultFb.innerHTML =
        '<div class="ios-feedback">✓ Face ID ok → E2E tunnel → MacBook .env (never in chat)</div>';
    });
  }

  // Sync iOS #4 Dynamic Island with desktop pill state
  var iosIsland = document.getElementById("ios-island-screen");
  var islandBtns = document.querySelectorAll("#island-states button");
  function renderIosIsland(mode) {
    if (!iosIsland) return;
    if (mode === "compact") {
      iosIsland.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill"><span>✳</span><span>68%</span><span style="color:#64b5ff">2:14</span></div>' +
        '<div class="ios-lock-clock"><div class="date">Real iOS Dynamic Island</div><div class="time">10:52</div></div>' +
        '<div class="ios-glass">' +
        '  <div class="di-row"><b>Claude · 412 tests</b><span style="font-family:var(--hk-mono);font-size:10px;color:#7fb7ff">68%</span></div>' +
        '  <div class="di-progress" style="margin-top:6px"><i style="width:68%"></i></div>' +
        '</div>';
    } else if (mode === "expanded") {
      iosIsland.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill expanded">' +
        '  <div class="di-row"><span>✳ <b>Claude</b> · herdr-gpui</span><span style="color:#7fb7ff">68%</span></div>' +
        '  <div style="font:10px/1.3 var(--hk-mono);color:#9ea7ba">✓ 281 passed · chat::effects</div>' +
        '  <div class="di-progress"><i style="width:68%"></i></div>' +
        '  <div class="ios-btn-row"><button class="ios-btn danger" type="button">Stop</button><button class="ios-btn primary" type="button">Open</button></div>' +
        '</div>' +
        '<div class="ios-note" style="color:#9ea7ba;margin-top:auto">Long-press Dynamic Island anywhere on iPhone</div>';
    } else if (mode === "multi") {
      iosIsland.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill expanded">' +
        '  <div class="di-row"><b>3 agents active</b><span style="color:#f6c978">1 blocked</span></div>' +
        '  <div style="font-size:10px;color:#cfd5e3">● Claude · tests (68%)<br>● Codex · site (done)<br>● Claude · billing (YubiKey)</div>' +
        '</div>';
    } else if (mode === "blocked") {
      iosIsland.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill expanded amber">' +
        '  <div class="di-row"><span style="color:#f6c978">🔑 BLOCKED · YUBIKEY</span><span>0:42</span></div>' +
        '  <div style="font-size:11.5px;font-weight:700">Tap YubiKey at MacBook Pro</div>' +
        '  <div style="font-size:10px;color:#c7cfdf">Claude · git push feat/chat-modals</div>' +
        '</div>' +
        '<div class="ios-haptic-pill" style="margin:auto auto 0">📳 Pulsing amber Live Activity</div>';
    } else {
      iosIsland.innerHTML =
        '<div class="iphone-status"><span>10:52</span><span>5G ▮</span></div>' +
        '<div class="di-pill green">✓ Pushed feat/chat-modals</div>' +
        '<div class="ios-lock-clock"><div class="date">Auto-dismissing Live Activity</div><div class="time">10:52</div></div>';
    }
  }
  islandBtns.forEach(function (b) {
    b.addEventListener("click", function () { renderIosIsland(b.getAttribute("data-island")); });
  });
  renderIosIsland("compact");

  // iOS #5 Runbook interactive checkbox
  var iosRbBtn = document.getElementById("ios-rb-check");
  var iosRbFb = document.getElementById("ios-rb-fb");
  if (iosRbBtn && iosRbFb) {
    iosRbBtn.addEventListener("click", function () {
      iosRbBtn.textContent = "✓ Checked — Claude resumed on Mac";
      iosRbFb.innerHTML = '<div class="ios-feedback">Synced over E2E tunnel (3 of 5)</div>';
    });
  }

  // iOS #6 Drop/Capture + Voice buttons
  var iosCapFb = document.getElementById("ios-cap-fb");
  document.querySelectorAll("[data-ios-cap]").forEach(function (b) {
    b.addEventListener("click", function () {
      if (iosCapFb) iosCapFb.innerHTML = '<div class="ios-feedback">✓ ' + b.getAttribute("data-ios-cap") + '</div>';
    });
  });

  // iOS #7 Spotlight / Mini-Diff toggle
  var iosSpotFb = document.getElementById("ios-md-fb");
  document.querySelectorAll("[data-ios-md]").forEach(function (b) {
    b.addEventListener("click", function () {
      if (iosSpotFb) iosSpotFb.innerHTML = '<div class="ios-feedback">✓ ' + b.getAttribute("data-ios-md") + '</div>';
    });
  });

  // Sync iOS #8 Cue preview with desktop audio/edge-light clicks
  var iosCueScreen = document.getElementById("ios-cue-screen");
  var iosCueLabel = document.getElementById("ios-cue-label");
  var iosCueHaptic = document.getElementById("ios-cue-haptic");
  var hapticMap = {
    yubikey: ["Amber double-heartbeat", "Cue.Tap: 2× crisp knock (0.0s, 0.14s)"],
    decision: ["Calm blue glow", "Cue.Tap: soft dual pulse (0.0s, 0.12s)"],
    secret: ["Indigo vault shimmer", "Cue.Tap: sharp lock click (0.0s, 0.09s)"],
    countdown: ["Pulsing coral edge", "Cue.Tap: light tick cadence"],
    done: ["Emerald sweep", "Cue.Tap: UINotificationFeedback .success"],
  };
  document.querySelectorAll("[data-cue]").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var id = btn.getAttribute("data-cue");
      if (!iosCueScreen || !hapticMap[id]) return;
      iosCueScreen.className = "iphone-screen ios-cue-" + id;
      iosCueLabel.textContent = hapticMap[id][0];
      iosCueHaptic.textContent = "📳 " + hapticMap[id][1];
    });
  });
})();
