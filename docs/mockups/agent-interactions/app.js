(function () {
  "use strict";

  // Theme toggle
  var themeBtn = document.getElementById("theme-toggle");
  if (themeBtn) {
    themeBtn.addEventListener("click", function () {
      var root = document.documentElement;
      var cur = root.getAttribute("data-theme");
      var prefersDark = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
      root.setAttribute("data-theme", cur ? (cur === "dark" ? "light" : "dark") : (prefersDark ? "light" : "dark"));
    });
  }

  // ═══════════════════════════════════════════════════════════════════
  // 01 · SMART ESCALATION LADDER
  // ═══════════════════════════════════════════════════════════════════
  var escData = [
    {
      kicker: "LEVEL 0 · YOU ARE LOOKING AT THIS CHAT",
      title: "Inline chip only — zero disruption",
      body: "Because your cursor and eyes are already in Claude's chat pane, Sheprd renders a pinned amber action bar inside the chat instead of throwing a modal in your face.",
      rule: "Rule: focused_window == chat_window && active_pane == chat_pane",
      visual:
        '<div class="esc-mock-card subtle">' +
        '  <div class="g-hud-meta"><span class="agent-pill claude">✳ Claude</span><span>Inline in active chat</span></div>' +
        '  <div style="font-weight:650;font-size:13px;margin-bottom:4px">🔑 Tap your YubiKey to finish git push</div>' +
        '  <div style="font-size:11.5px;color:#d8c39b">Auto-clears the moment git push unblocks.</div>' +
        '</div>',
    },
    {
      kicker: "LEVEL 1 · SAME SHEPRD WINDOW, DIFFERENT TAB",
      title: "Persistent Window Modal",
      body: "You switched to Neovim or another worktree inside the same Sheprd window. A persistent sheet pins over this window with amber Edge Light until you jump or dismiss.",
      rule: "Rule: focused_window == chat_window && active_pane != chat_pane",
      visual:
        '<div class="esc-mock-card">' +
        '  <div class="g-hud-meta"><span class="agent-pill claude">✳ Claude</span><span>Window Modal · over nvim</span></div>' +
        '  <div style="font-weight:650;font-size:13.5px;margin-bottom:5px">Tap your YubiKey to sign &amp; push</div>' +
        '  <div style="font-size:11.5px;color:#aeb6c8;margin-bottom:10px">Blocked on <code>git push origin feat/modals</code></div>' +
        '  <div style="display:flex;justify-content:flex-end;gap:6px"><span class="m-btn ghost xs">Dismiss</span><span class="m-btn primary xs">Go to chat</span></div>' +
        '</div>',
    },
    {
      kicker: "LEVEL 2 · SHEPRD IN BACKGROUND / OTHER SPACE",
      title: "Global Always-on-Top HUD + Dock Bounce",
      body: "You're reading a PR in Safari or on another fullscreen Space. An always-on-top NSPanel floats above every app without stealing keyboard focus, and Sheprd's Dock icon bounces.",
      rule: "Rule: !app_is_frontmost && priority == urgent (or unread > 10s)",
      visual:
        '<div class="esc-mock-card amber">' +
        '  <div class="g-hud-meta"><span class="sheprd-chip">Sheprd · All Apps</span><span>Claude · BLOCKED 0:12</span></div>' +
        '  <div style="font-weight:700;font-size:14px;margin-bottom:5px">🔑 Tap your YubiKey now</div>' +
        '  <div style="font-size:11.5px;color:#cfd6e6;margin-bottom:10px">Floats over Safari &amp; all Spaces · keeps your typing focus</div>' +
        '  <div style="display:flex;justify-content:flex-end;gap:6px"><span class="m-btn ghost xs">Snooze 2m</span><span class="m-btn primary xs">Open Sheprd</span></div>' +
        '</div>',
    },
    {
      kicker: "LEVEL 3 · NO INPUT ON MAC FOR 20s",
      title: "Escalate to Paired iPhone (Live Activity + Push)",
      body: "You stepped away for coffee. Sheprd sends a time-sensitive push and Live Activity to your iPhone noting that physical touch is needed at your MacBook Pro.",
      rule: "Rule: hid_idle_secs >= 20 && paired_phone_reachable",
      visual:
        '<div class="esc-phone-mock">' +
        '  <div class="esc-phone-island">● Tap YubiKey at Mac</div>' +
        '  <div style="font-size:20px;font-weight:700;margin-bottom:8px">10:52</div>' +
        '  <div class="esc-phone-push">' +
        '    <div style="color:#f6c978;font-size:9px;font-weight:700;letter-spacing:.05em">SHEPRD · TIME SENSITIVE</div>' +
        '    <div style="font-weight:700;margin:2px 0">Tap YubiKey at MacBook Pro</div>' +
        '    <div style="color:#c1c8d8">Claude is blocked on git push (22s)</div>' +
        '  </div>' +
        '</div>',
    },
  ];

  var escStage = document.getElementById("esc-stage");
  var escBtns = document.querySelectorAll("#esc-steps button");
  var escTimer = null;

  function renderEsc(idx) {
    var d = escData[idx];
    escBtns.forEach(function (b) { b.classList.toggle("active", Number(b.getAttribute("data-step")) === idx); });
    if (!escStage) return;
    escStage.innerHTML =
      '<div class="esc-visual">' + d.visual + '</div>' +
      '<div class="esc-explain">' +
      '  <div class="kicker">' + d.kicker + '</div>' +
      '  <h4>' + d.title + '</h4>' +
      '  <p>' + d.body + '</p>' +
      '  <div class="esc-rule">' + d.rule + '</div>' +
      '</div>';
  }
  escBtns.forEach(function (b) {
    b.addEventListener("click", function () {
      clearInterval(escTimer);
      renderEsc(Number(b.getAttribute("data-step")));
    });
  });
  var escAuto = document.getElementById("esc-autoplay");
  if (escAuto) {
    escAuto.addEventListener("click", function () {
      clearInterval(escTimer);
      var i = 0;
      renderEsc(0);
      escTimer = setInterval(function () {
        i += 1;
        if (i >= escData.length) { clearInterval(escTimer); return; }
        renderEsc(i);
      }, 1500);
    });
  }
  renderEsc(0);

  // ═══════════════════════════════════════════════════════════════════
  // 02 · ACTION-CAPABLE & TYPED MODALS
  // ═══════════════════════════════════════════════════════════════════
  var presetStage = document.getElementById("preset-stage");
  var presetBtns = document.querySelectorAll("#preset-tabs button");
  var cdInterval = null;

  function renderPreset(kind) {
    clearInterval(cdInterval);
    presetBtns.forEach(function (b) { b.classList.toggle("active", b.getAttribute("data-preset") === kind); });
    if (!presetStage) return;

    if (kind === "choices") {
      presetStage.innerHTML =
        '<div class="typed-card">' +
        '  <div class="typed-meta"><span class="agent-pill claude">✳ Claude</span><span>herdr-gpui · PR #1428</span></div>' +
        '  <h4>How should I handle the 3 failing snapshot tests?</h4>' +
        '  <p>Layout padding shifted by 2px after the header refactor. Pick one — your choice goes straight to Claude.</p>' +
        '  <div class="typed-actions">' +
        '    <button class="m-btn ghost sm" type="button" data-pick-act="Opened diff tab">Open diff</button>' +
        '    <button class="m-btn ghost sm" type="button" data-pick-act="Sent: Revert header padding">Revert padding</button>' +
        '    <button class="m-btn primary sm" type="button" data-pick-act="Sent: Update snapshots & push">Update snapshots</button>' +
        '  </div>' +
        '  <div id="typed-fb"></div>' +
        '</div>';
      presetStage.querySelectorAll("[data-pick-act]").forEach(function (btn) {
        btn.addEventListener("click", function () {
          document.getElementById("typed-fb").innerHTML =
            '<div class="typed-feedback">✓ ' + btn.getAttribute("data-pick-act") + '</div>';
        });
      });
    } else if (kind === "countdown") {
      var remaining = 15.0;
      var paused = false;
      presetStage.innerHTML =
        '<div class="typed-card amber">' +
        '  <div class="typed-meta"><span class="agent-pill codex">◎ Codex</span><span>Auto-proceed countdown</span><span id="cd-sec" style="margin-left:auto;color:#f8ca70;font-weight:700">15s</span></div>' +
        '  <h4>Deploying <code>staging-eu-1</code> in <span id="cd-inline">15</span>s</h4>' +
        '  <p>All 214 tests passed. Proceeding automatically unless you hit Cancel.</p>' +
        '  <div class="countdown-track"><div class="countdown-fill" id="cd-fill" style="width:100%"></div></div>' +
        '  <div class="typed-actions">' +
        '    <button class="m-btn danger sm" type="button" id="cd-cancel">Cancel deploy</button>' +
        '    <button class="m-btn ghost sm" type="button" id="cd-pause">Pause</button>' +
        '    <button class="m-btn primary sm" type="button" id="cd-now">Run now</button>' +
        '  </div>' +
        '  <div id="typed-fb"></div>' +
        '</div>';
      cdInterval = setInterval(function () {
        if (paused) return;
        remaining = Math.max(0, remaining - 0.1);
        var secs = Math.ceil(remaining);
        var sEl = document.getElementById("cd-sec");
        var inEl = document.getElementById("cd-inline");
        var fEl = document.getElementById("cd-fill");
        if (sEl) sEl.textContent = secs + "s";
        if (inEl) inEl.textContent = String(secs);
        if (fEl) fEl.style.width = (remaining / 15 * 100) + "%";
        if (remaining <= 0) {
          clearInterval(cdInterval);
          var fb = document.getElementById("typed-fb");
          if (fb) fb.innerHTML = '<div class="typed-feedback">✓ Countdown reached 0 — deploy started</div>';
        }
      }, 100);
      document.getElementById("cd-cancel").addEventListener("click", function () {
        clearInterval(cdInterval);
        document.getElementById("typed-fb").innerHTML = '<div class="typed-feedback">✓ Cancelled — Codex notified to stop</div>';
      });
      document.getElementById("cd-pause").addEventListener("click", function (e) {
        paused = !paused;
        e.target.textContent = paused ? "Resume" : "Pause";
      });
      document.getElementById("cd-now").addEventListener("click", function () {
        clearInterval(cdInterval);
        document.getElementById("cd-fill").style.width = "0%";
        document.getElementById("typed-fb").innerHTML = '<div class="typed-feedback">✓ Running deploy immediately</div>';
      });
    } else if (kind === "otp") {
      presetStage.innerHTML =
        '<div class="typed-card amber">' +
        '  <div class="typed-meta"><span class="sheprd-chip">Sheprd · Global HUD</span><span>npm publish · 2FA required</span></div>' +
        '  <h4>Enter 6-digit authenticator code</h4>' +
        '  <p>Typed directly to the waiting CLI stdin — masked &amp; never stored in transcript.</p>' +
        '  <div class="otp-row" id="otp-row">' +
        '    <input class="otp-box" maxlength="1" inputmode="numeric"><input class="otp-box" maxlength="1" inputmode="numeric">' +
        '    <input class="otp-box" maxlength="1" inputmode="numeric"><input class="otp-box" maxlength="1" inputmode="numeric">' +
        '    <input class="otp-box" maxlength="1" inputmode="numeric"><input class="otp-box" maxlength="1" inputmode="numeric">' +
        '  </div>' +
        '  <div class="typed-actions">' +
        '    <button class="m-btn ghost sm" type="button" id="otp-fill">Fill demo code</button>' +
        '    <button class="m-btn primary sm" type="button" id="otp-submit">Submit OTP</button>' +
        '  </div>' +
        '  <div id="typed-fb"></div>' +
        '</div>';
      var boxes = Array.prototype.slice.call(presetStage.querySelectorAll(".otp-box"));
      boxes.forEach(function (box, idx) {
        box.addEventListener("input", function () {
          if (box.value && idx < boxes.length - 1) boxes[idx + 1].focus();
        });
      });
      document.getElementById("otp-fill").addEventListener("click", function () {
        "482910".split("").forEach(function (d, i) { boxes[i].value = d; });
      });
      document.getElementById("otp-submit").addEventListener("click", function () {
        document.getElementById("typed-fb").innerHTML =
          '<div class="typed-feedback">✓ Sent 6-digit OTP to npm publish stdin (redacted from chat)</div>';
      });
    } else {
      presetStage.innerHTML =
        '<div class="typed-card amber">' +
        '  <div class="typed-meta"><span class="sheprd-chip">Sheprd · Global HUD</span><span>gh auth refresh · OAuth</span></div>' +
        '  <h4>Complete GitHub SSO in your browser</h4>' +
        '  <p>One-time code <code>A4F9-2C81</code> copied to clipboard. This HUD auto-clears once the local callback returns.</p>' +
        '  <div class="typed-actions">' +
        '    <button class="m-btn ghost sm" type="button" id="oauth-copy">Copy code</button>' +
        '    <button class="m-btn primary sm" type="button" id="oauth-open">Open github.com/login/device ↗</button>' +
        '  </div>' +
        '  <div id="typed-fb"></div>' +
        '</div>';
      document.getElementById("oauth-copy").addEventListener("click", function () {
        document.getElementById("typed-fb").innerHTML = '<div class="typed-feedback">Copied A4F9-2C81</div>';
      });
      document.getElementById("oauth-open").addEventListener("click", function () {
        document.getElementById("typed-fb").innerHTML = '<div class="typed-feedback">Waiting for callback…</div>';
        setTimeout(function () {
          var fb = document.getElementById("typed-fb");
          if (fb) fb.innerHTML = '<div class="typed-feedback">✓ OAuth callback received — HUD auto-cleared!</div>';
        }, 900);
      });
    }
  }
  presetBtns.forEach(function (b) {
    b.addEventListener("click", function () { renderPreset(b.getAttribute("data-preset")); });
  });
  renderPreset("choices");

  // ═══════════════════════════════════════════════════════════════════
  // 03 · SECRET-SAFE PROMPT MODAL
  // ═══════════════════════════════════════════════════════════════════
  var vInput = document.getElementById("vault-input");
  var vReveal = document.getElementById("vault-reveal");
  var vKc = document.getElementById("vault-keychain");
  var vSubmit = document.getElementById("vault-submit");
  var vCancel = document.getElementById("vault-cancel");
  var sLocal = document.getElementById("shield-local");
  var sLlm = document.getElementById("shield-llm");

  if (vReveal && vInput) {
    vReveal.addEventListener("click", function () {
      var hidden = vInput.type === "password";
      vInput.type = hidden ? "text" : "password";
      vReveal.textContent = hidden ? "Hide" : "Show";
    });
  }
  if (vKc && vInput) {
    vKc.addEventListener("click", function () {
      vInput.value = "sk_test_keychain_7f9a2c44e8b1";
      sLocal.textContent = "Loaded from macOS Keychain (Touch ID verified).\nReady to inject into .env.test.local.";
    });
  }
  if (vSubmit) {
    vSubmit.addEventListener("click", function () {
      var len = vInput ? vInput.value.length : 0;
      var saveKc = document.getElementById("vault-save-kc").checked;
      sLocal.textContent =
        "✓ Written atomically to ~/code/billing-api/.env.test.local (mode 0600)\n" +
        "  STRIPE_TEST_KEY=<" + len + " bytes written directly by Sheprd>\n" +
        (saveKc ? "✓ Saved in macOS Keychain (service: sheprd.secret.billing-api)" : "• Not saved to Keychain (one-time use)");
      sLlm.textContent =
        "✓ [sheprd-vault] User provided STRIPE_TEST_KEY.\n" +
        "  Written to .env.test.local. Plaintext value was NEVER\n" +
        "  sent to Claude or recorded in the chat transcript.";
    });
  }
  if (vCancel) {
    vCancel.addEventListener("click", function () {
      sLocal.textContent = "Denied — nothing written to disk or Keychain.";
      sLlm.textContent = "[sheprd-vault] User declined request for STRIPE_TEST_KEY.";
    });
  }

  // ═══════════════════════════════════════════════════════════════════
  // 04 · DYNAMIC PROGRESS PILL ("DYNAMIC ISLAND")
  // ═══════════════════════════════════════════════════════════════════
  var islandAnchor = document.getElementById("island-anchor");
  var islandDesktop = document.getElementById("island-desktop");
  var islandBtns = document.querySelectorAll("#island-states button");
  var islandPosBtns = document.querySelectorAll("#island-pos button");
  var islandTimer = null;

  function renderIsland(mode) {
    islandBtns.forEach(function (b) { b.classList.toggle("active", b.getAttribute("data-island") === mode); });
    if (!islandAnchor) return;

    if (mode === "compact") {
      islandAnchor.innerHTML =
        '<div class="dpill compact">' +
        '  <span class="agent-pill claude" style="padding:2px 6px">✳</span>' +
        '  <span><b>Claude</b> · 412 tests</span>' +
        '  <span class="pill-mini-bar"><i style="width:68%"></i></span>' +
        '  <span style="font-family:var(--hk-mono);font-size:11px;color:#9ea7ba">68% · 2:14</span>' +
        '</div>';
    } else if (mode === "expanded") {
      islandAnchor.innerHTML =
        '<div class="dpill expanded">' +
        '  <div class="pill-row"><span><span class="agent-pill claude" style="padding:2px 6px">✳</span> <b>Claude</b> · herdr-gpui</span><span style="font-family:var(--hk-mono);font-size:11px;color:#7fb7ff">68% · 2:14</span></div>' +
        '  <div class="pill-step">✓ 281 passed · ▸ chat::effects::tests</div>' +
        '  <div class="countdown-track" style="margin-bottom:10px"><div style="width:68%;height:100%;background:#64b5ff"></div></div>' +
        '  <div style="display:flex;justify-content:space-between;gap:6px">' +
        '    <button class="m-btn danger xs" type="button" data-is-act="compact">Stop</button>' +
        '    <button class="m-btn primary xs" type="button" data-is-act="compact">Open chat ↗</button>' +
        '  </div>' +
        '</div>';
    } else if (mode === "multi") {
      islandAnchor.innerHTML =
        '<div class="dpill expanded">' +
        '  <div class="pill-row"><span><b>3 agents running</b></span><span style="font-family:var(--hk-mono);font-size:11px;color:#9ea7ba">2/3 finishing</span></div>' +
        '  <div class="pill-agents">' +
        '    <div class="pill-agent-row"><span class="cue-dot blue"></span><span>Claude · herdr-gpui (tests)</span><span class="pct">68%</span></div>' +
        '    <div class="pill-agent-row"><span class="cue-dot green"></span><span>Codex · site (build)</span><span class="pct">✓ done</span></div>' +
        '    <div class="pill-agent-row"><span class="cue-dot amber"></span><span>Claude · billing (push)</span><span class="pct" style="color:#f6c978">blocked</span></div>' +
        '  </div>' +
        '</div>';
    } else if (mode === "blocked") {
      islandAnchor.innerHTML =
        '<div class="dpill blocked">' +
        '  <div class="pill-row" style="margin-bottom:6px"><span class="sheprd-chip">Sheprd · Morph</span><span style="color:#f8ca70;font:700 10.5px/1 var(--hk-mono)">BLOCKED · YUBIKEY</span></div>' +
        '  <div style="font-size:14px;font-weight:700;margin-bottom:3px">🔑 Tap your YubiKey to push</div>' +
        '  <div style="font-size:11.5px;color:#bcc4d6;margin-bottom:10px">Claude · <code>git push origin feat/chat-modals</code></div>' +
        '  <div style="display:flex;justify-content:flex-end;gap:6px">' +
        '    <button class="m-btn ghost xs" type="button" data-is-act="done">✦ Simulate YubiKey tap</button>' +
        '    <button class="m-btn primary xs" type="button" data-is-act="compact">Open chat</button>' +
        '  </div>' +
        '</div>';
    } else {
      islandAnchor.innerHTML =
        '<div class="dpill done">✓ YubiKey tapped · Pushed feat/chat-modals (auto-hiding…)</div>';
    }

    islandAnchor.querySelectorAll("[data-is-act]").forEach(function (b) {
      b.addEventListener("click", function () { renderIsland(b.getAttribute("data-is-act")); });
    });
  }

  islandBtns.forEach(function (b) {
    b.addEventListener("click", function () {
      clearInterval(islandTimer);
      renderIsland(b.getAttribute("data-island"));
    });
  });
  islandPosBtns.forEach(function (b) {
    b.addEventListener("click", function () {
      var pos = b.getAttribute("data-pos");
      islandPosBtns.forEach(function (x) { x.classList.toggle("active", x === b); });
      islandDesktop.classList.toggle("pos-top", pos === "top");
      islandDesktop.classList.toggle("pos-corner", pos === "corner");
    });
  });
  var islandPlay = document.getElementById("island-play");
  if (islandPlay) {
    islandPlay.addEventListener("click", function () {
      clearInterval(islandTimer);
      var seq = ["compact", "expanded", "blocked", "done"];
      var i = 0;
      renderIsland(seq[0]);
      islandTimer = setInterval(function () {
        i += 1;
        if (i >= seq.length) { clearInterval(islandTimer); return; }
        renderIsland(seq[i]);
      }, 1500);
    });
  }
  renderIsland("compact");

  // ═══════════════════════════════════════════════════════════════════
  // 05 · SHARED RUNBOOK CHECKLIST
  // ═══════════════════════════════════════════════════════════════════
  var rbYubi = document.querySelector('[data-rb-step="yubi"]');
  var rbWatch = document.querySelector('[data-rb-step="ci-watch"]');
  var rbPub = document.querySelector('[data-rb-step="publish"]');
  var rbCount = document.getElementById("rb-count");
  var rbFill = document.getElementById("rb-fill");
  var rbStatus = document.getElementById("rb-status");
  var rbReset = document.getElementById("rb-reset");
  var rbTimer = null;

  function setRbProgress(done) {
    if (rbCount) rbCount.textContent = done + " of 5";
    if (rbFill) rbFill.style.width = (done / 5 * 100) + "%";
  }
  if (rbYubi) {
    var yBtn = rbYubi.querySelector(".rb-check-btn");
    yBtn.addEventListener("click", function () {
      if (yBtn.classList.contains("checked")) return;
      yBtn.classList.add("checked");
      yBtn.textContent = "✓";
      rbYubi.classList.remove("waiting");
      rbYubi.classList.add("done");
      rbYubi.querySelector(".rb-owner").textContent = "You · done";
      setRbProgress(3);
      rbWatch.classList.remove("locked");
      rbWatch.classList.add("working");
      rbWatch.querySelector(".rb-check").textContent = "◌";
      rbWatch.querySelector(".rb-owner").textContent = "Claude · running…";
      rbStatus.innerHTML = "Your step completed! <b>Claude resumed</b> watching GitHub release CI…";
      rbTimer = setTimeout(function () {
        rbWatch.classList.remove("working");
        rbWatch.classList.add("done");
        rbWatch.querySelector(".rb-check").textContent = "✓";
        rbWatch.querySelector(".rb-check").classList.add("checked");
        rbWatch.querySelector(".rb-owner").textContent = "Claude · done";
        setRbProgress(4);
        rbPub.classList.remove("locked");
        rbPub.classList.add("waiting");
        var pBtn = rbPub.querySelector(".rb-check-btn");
        pBtn.disabled = false;
        rbPub.querySelector(".rb-owner").textContent = "You · click to publish";
        rbStatus.innerHTML = "DMG signed &amp; verified. Waiting on your final approval!";
      }, 1100);
    });
  }
  if (rbPub) {
    var pBtn = rbPub.querySelector(".rb-check-btn");
    pBtn.addEventListener("click", function () {
      if (pBtn.classList.contains("checked")) return;
      pBtn.classList.add("checked");
      pBtn.textContent = "✓";
      rbPub.classList.remove("waiting");
      rbPub.classList.add("done");
      rbPub.querySelector(".rb-owner").textContent = "You · published";
      setRbProgress(5);
      rbStatus.innerHTML = "✓ <b>v0.18.0 shipped!</b> Runbook auto-archived.";
    });
  }
  if (rbReset) {
    rbReset.addEventListener("click", function () {
      clearTimeout(rbTimer);
      setRbProgress(2);
      var yBtn = rbYubi.querySelector(".rb-check-btn");
      yBtn.classList.remove("checked"); yBtn.textContent = "";
      rbYubi.className = "rb-item human waiting";
      rbYubi.querySelector(".rb-owner").textContent = "You · click to complete";
      rbWatch.className = "rb-item locked";
      rbWatch.querySelector(".rb-check").textContent = "○";
      rbWatch.querySelector(".rb-check").classList.remove("checked");
      rbWatch.querySelector(".rb-owner").textContent = "Claude · waiting";
      var pBtn = rbPub.querySelector(".rb-check-btn");
      pBtn.classList.remove("checked"); pBtn.textContent = ""; pBtn.disabled = true;
      rbPub.className = "rb-item human locked";
      rbPub.querySelector(".rb-owner").textContent = "You";
      rbStatus.innerHTML = "Waiting on your step: <b>Tap YubiKey</b>";
    });
  }

  // ═══════════════════════════════════════════════════════════════════
  // ROADMAP PRIORITY PICKER + SEND PICKS
  // ═══════════════════════════════════════════════════════════════════
  var roadmap = {
    escalation: "p1",
    typed_modals: "p1",
    secret_modal: "p1",
    progress_pill: "p1",
    runbook: "p1",
    drop_voice: "p1",
    spotlight_diff: "p1",
    audio_edge: "p1",
  };
  var rfSummary = document.getElementById("rf-summary");
  function updateRoadmapSummary() {
    var p1 = 0, p2 = 0, later = 0;
    Object.keys(roadmap).forEach(function (k) {
      if (roadmap[k] === "p1") p1++;
      else if (roadmap[k] === "p2") p2++;
      else later++;
    });
    if (rfSummary) {
      rfSummary.textContent = "Phase 1: " + p1 + " · Phase 2: " + p2 + (later ? " · Later: " + later : "");
    }
  }
  document.querySelectorAll("[data-phase-group]").forEach(function (group) {
    var cap = group.getAttribute("data-phase-group");
    group.querySelectorAll("button").forEach(function (btn) {
      btn.addEventListener("click", function () {
        roadmap[cap] = btn.getAttribute("data-phase");
        group.querySelectorAll("button").forEach(function (b) { b.classList.toggle("active", b === btn); });
        updateRoadmapSummary();
      });
    });
  });
  var sendRoadmap = document.getElementById("send-roadmap");
  if (sendRoadmap) {
    sendRoadmap.addEventListener("click", function () {
      var p1List = Object.keys(roadmap).filter(function (k) { return roadmap[k] === "p1"; });
      var summary = "Agent interactions roadmap — Phase 1: " + p1List.join(", ");
      if (window.HerdrKit && window.HerdrKit.send) {
        window.HerdrKit.send(summary, { roadmap: roadmap });
      } else {
        alert(summary + "\n" + JSON.stringify(roadmap, null, 2));
      }
    });
  }
  updateRoadmapSummary();
})();
