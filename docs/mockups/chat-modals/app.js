(function () {
  "use strict";

  var state = {
    window_modal: null,
    global_modal: null,
    decisions: {
      trigger: "both",
      focus: "no_steal",
      autoclear: "auto",
      settings: "separate",
    },
  };

  // Theme toggle
  var themeBtn = document.getElementById("theme-toggle");
  if (themeBtn) {
    themeBtn.addEventListener("click", function () {
      var root = document.documentElement;
      var current = root.getAttribute("data-theme");
      var prefersDark = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
      var next = current ? (current === "dark" ? "light" : "dark") : (prefersDark ? "light" : "dark");
      root.setAttribute("data-theme", next);
    });
  }

  // Variant picking
  function updateSummary() {
    var el = document.getElementById("picks-summary");
    if (!el) return;
    el.textContent = "";
    el.appendChild(document.createTextNode("Current picks — Window: "));
    var w = document.createElement("strong");
    w.textContent = state.window_modal || "none";
    el.appendChild(w);
    el.appendChild(document.createTextNode(" · Global: "));
    var g = document.createElement("strong");
    g.textContent = state.global_modal || "none";
    el.appendChild(g);
    el.appendChild(document.createTextNode(" · Trigger: "));
    var t = document.createElement("strong");
    t.textContent = state.decisions.trigger === "both" ? "Directives + CLI" : "Directives only";
    el.appendChild(t);
  }

  document.querySelectorAll(".variants-grid").forEach(function (grid) {
    var group = grid.getAttribute("data-group");
    grid.querySelectorAll("[data-pick]").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var variant = btn.getAttribute("data-pick");
        state[group] = variant;
        grid.querySelectorAll(".variant-card").forEach(function (card) {
          var isPicked = card.getAttribute("data-variant") === variant;
          card.classList.toggle("picked", isPicked);
          var cardBtn = card.querySelector("[data-pick]");
          if (cardBtn) {
            cardBtn.textContent = isPicked ? "Picked " + variant + " ✓" : "Pick " + card.getAttribute("data-variant");
          }
        });
        updateSummary();
      });
    });
  });

  // Decision choices
  document.querySelectorAll(".dec-card").forEach(function (card) {
    var key = card.getAttribute("data-decision");
    card.querySelectorAll(".dec-choices button").forEach(function (btn) {
      btn.addEventListener("click", function () {
        state.decisions[key] = btn.getAttribute("data-val");
        card.querySelectorAll(".dec-choices button").forEach(function (b) {
          b.classList.toggle("selected", b === btn);
        });
        updateSummary();
      });
    });
  });

  // Mini frame interactions (dismiss, jump, reply, simulate YubiKey tap, replay)
  function hideModalIn(frame, toastText) {
    frame.querySelectorAll("[data-modal-el], [data-modal-el-mirror]").forEach(function (el) {
      el.style.opacity = "0";
      el.style.pointerEvents = "none";
    });
    frame.classList.remove("edge-amber");
    var bounce = frame.querySelector(".sheprd-dock");
    if (bounce) bounce.classList.remove("bouncing");
    if (toastText) {
      var pill = document.createElement("div");
      pill.className = "cleared-pill";
      pill.textContent = toastText;
      frame.appendChild(pill);
      setTimeout(function () {
        pill.remove();
      }, 1700);
    }
  }

  function resetModalIn(frame) {
    frame.querySelectorAll(".cleared-pill").forEach(function (p) { p.remove(); });
    frame.querySelectorAll("[data-modal-el], [data-modal-el-mirror]").forEach(function (el) {
      el.style.opacity = "";
      el.style.pointerEvents = "";
    });
    if (frame.getAttribute("data-demo") === "w2") frame.classList.add("edge-amber");
    var bounce = frame.querySelector(".sheprd-dock");
    if (bounce) bounce.classList.add("bouncing");
  }

  document.querySelectorAll("[data-demo]").forEach(function (frame) {
    frame.querySelectorAll("[data-action]").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var action = btn.getAttribute("data-action");
        var msg =
          action === "jump" ? "Focused Claude chat tab ↗" :
          action === "reply" ? "Reply sent to Claude ✓" :
          action === "snooze" ? "Snoozed for 2 minutes" :
          "Dismissed";
        hideModalIn(frame, msg);
      });
    });
  });

  document.querySelectorAll("[data-reset]").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var id = btn.getAttribute("data-reset");
      var frame = document.querySelector('[data-demo="' + id + '"]');
      if (frame) resetModalIn(frame);
    });
  });

  document.querySelectorAll("[data-yubikey]").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var id = btn.getAttribute("data-yubikey");
      var frame = document.querySelector('[data-demo="' + id + '"]');
      if (frame) hideModalIn(frame, "✓ YubiKey tapped — push unblocked & modal cleared");
    });
  });

  // Inline chat chip toggle
  var chip = document.getElementById("chat-inline-chip");
  var chipBtn = document.getElementById("toggle-chip-state");
  if (chip && chipBtn) {
    chipBtn.addEventListener("click", function () {
      var dismissed = chip.classList.toggle("dismissed");
      var title = chip.querySelector("strong");
      if (title) {
        title.textContent = dismissed
          ? "Global modal dismissed · Tap your YubiKey"
          : "Global modal active · Tap your YubiKey";
      }
      chipBtn.textContent = dismissed ? "Reopen" : "Dismiss";
    });
  }

  // Stacked deck interactive pager
  var stackItems = [
    {
      badge: "✳",
      badgeClass: "claude",
      who: "Claude · herdr-gpui",
      title: "Tap your YubiKey to sign & push",
      body: "Waiting on git push origin feat/chat-modals · blocked 42s",
    },
    {
      badge: "◎",
      badgeClass: "codex",
      who: "Codex · billing-api",
      title: "Approve destructive migration?",
      body: "Ready to drop legacy_invoices table in staging DB · waiting 1m 05s",
    },
  ];
  var stackIdx = 0;
  function renderStack() {
    var item = stackItems[stackIdx];
    var badge = document.getElementById("stack-badge");
    var who = document.getElementById("stack-who");
    var title = document.getElementById("stack-title");
    var body = document.getElementById("stack-body");
    var count = document.getElementById("stack-count");
    if (!badge) return;
    badge.textContent = item.badge;
    badge.className = "agent-badge " + item.badgeClass;
    who.textContent = item.who;
    title.textContent = item.title;
    body.textContent = item.body;
    count.textContent = (stackIdx + 1) + " of " + stackItems.length;
  }
  var prevBtn = document.getElementById("stack-prev");
  var nextBtn = document.getElementById("stack-next");
  if (prevBtn && nextBtn) {
    prevBtn.addEventListener("click", function () {
      stackIdx = (stackIdx + stackItems.length - 1) % stackItems.length;
      renderStack();
    });
    nextBtn.addEventListener("click", function () {
      stackIdx = (stackIdx + 1) % stackItems.length;
      renderStack();
    });
  }

  // Live full-viewport overlays
  var liveHost = document.getElementById("live-overlay-host");
  function closeLive() {
    if (!liveHost) return;
    liveHost.hidden = true;
    liveHost.className = "live-overlay-host";
    liveHost.textContent = "";
  }
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape" && liveHost && !liveHost.hidden) closeLive();
  });

  var fireWin = document.getElementById("fire-live-window");
  if (fireWin && liveHost) {
    fireWin.addEventListener("click", function () {
      liveHost.hidden = false;
      liveHost.className = "live-overlay-host scrimmed";
      liveHost.innerHTML =
        '<div class="m-card m-card-center" role="dialog" aria-modal="true">' +
        '  <div class="m-source"><span class="agent-badge claude">✳</span><span class="m-who"><strong>Claude</strong> · herdr-gpui / release</span><span class="m-timer">1m 14s</span></div>' +
        '  <h4 class="m-title">Confirm force-push to <code>feat/chat-modals</code>?</h4>' +
        '  <p class="m-body">Rebased 4 commits onto <code>main</code> cleanly. This is a persistent same-window modal — it stays until you act or press Esc.</p>' +
        '  <div class="m-actions">' +
        '    <button class="m-btn ghost" type="button" data-live-close>Dismiss <kbd>Esc</kbd></button>' +
        '    <button class="m-btn primary" type="button" data-live-close>Go to chat <kbd>↵</kbd></button>' +
        '  </div>' +
        '</div>';
      liveHost.querySelectorAll("[data-live-close]").forEach(function (b) {
        b.addEventListener("click", closeLive);
      });
    });
  }

  var fireGlobal = document.getElementById("fire-live-global");
  if (fireGlobal && liveHost) {
    fireGlobal.addEventListener("click", function () {
      liveHost.hidden = false;
      liveHost.className = "live-overlay-host hud-mode";
      liveHost.innerHTML =
        '<div class="g-hud-card" role="alertdialog">' +
        '  <div class="yubi-icon-box"><span class="yubi-ring"></span><span class="yubi-glyph">🔑</span></div>' +
        '  <div class="g-hud-copy">' +
        '    <div class="g-hud-meta"><span class="sheprd-chip">Sheprd · All Windows</span><span>Claude · <b>herdr-gpui</b></span><span class="blocked-tag">BLOCKED · 0:42</span></div>' +
        '    <div class="g-hud-title">Tap your YubiKey to sign &amp; push</div>' +
        '    <div class="g-hud-sub">Waiting on <code>git push origin feat/chat-modals</code> — floats over every app without stealing focus</div>' +
        '  </div>' +
        '  <div class="g-hud-actions">' +
        '    <button class="m-btn primary sm" type="button" data-live-close>Open chat</button>' +
        '    <button class="m-btn ghost sm" type="button" data-live-close>Dismiss</button>' +
        '  </div>' +
        '</div>';
      liveHost.querySelectorAll("[data-live-close]").forEach(function (b) {
        b.addEventListener("click", closeLive);
      });
    });
  }

  // Send picks back to agent via HerdrKit.send
  var sendBtn = document.getElementById("send-picks");
  if (sendBtn) {
    sendBtn.addEventListener("click", function () {
      var summary =
        "Chat modals mockup picks — Window: " +
        (state.window_modal || "unpicked") +
        ", Global: " +
        (state.global_modal || "unpicked");
      if (window.HerdrKit && window.HerdrKit.send) {
        window.HerdrKit.send(summary, state);
      } else if (window.herdr && window.herdr.send) {
        window.herdr.send(summary, state);
      } else {
        alert(summary + "\n" + JSON.stringify(state, null, 2));
      }
    });
  }

  updateSummary();
})();
