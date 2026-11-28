(function () {
  "use strict";

  // ═══════════════════════════════════════════════════════════════════
  // 06A · FLOATING DROP-ZONE HUD
  // ═══════════════════════════════════════════════════════════════════
  var well = document.getElementById("drop-well");
  var dTitle = document.getElementById("drop-title");
  var dSub = document.getElementById("drop-sub");
  var simDrop = document.getElementById("sim-drop");
  var resetDrop = document.getElementById("reset-drop");

  function showDropped(name, size) {
    if (!well) return;
    well.classList.remove("dragover");
    well.classList.add("dropped");
    dTitle.textContent = "✓ Attached " + name + " (" + size + ")";
    dSub.textContent = "Sent straight to Claude's chat — HUD auto-dismissing";
  }

  if (well) {
    well.addEventListener("dragover", function (e) {
      e.preventDefault();
      well.classList.add("dragover");
    });
    well.addEventListener("dragleave", function () {
      well.classList.remove("dragover");
    });
    well.addEventListener("drop", function (e) {
      e.preventDefault();
      var f = e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files[0];
      if (f) showDropped(f.name, Math.max(1, Math.round(f.size / 1024)) + " KB");
      else showDropped("dropped-item.txt", "4 KB");
    });
  }
  if (simDrop) {
    simDrop.addEventListener("click", function () {
      showDropped("crash-2026-11-26.ips", "42 KB");
    });
  }
  if (resetDrop) {
    resetDrop.addEventListener("click", function () {
      well.classList.remove("dropped", "dragover");
      dTitle.textContent = "Drop crash log or screenshot here";
      dSub.textContent = "Drag any real file onto this box, or simulate one below";
    });
  }

  // ═══════════════════════════════════════════════════════════════════
  // 06B · VOICE QUICK-REPLY HUD
  // ═══════════════════════════════════════════════════════════════════
  var voiceBtn = document.getElementById("voice-btn");
  var voiceSend = document.getElementById("voice-send");
  var voiceBars = document.getElementById("voice-bars");
  var voiceTx = document.getElementById("voice-transcript");
  var voiceTimer = null;

  if (voiceBtn) {
    voiceBtn.addEventListener("click", function () {
      clearInterval(voiceTimer);
      var words = ["Yeah,", "skip", "the", "flaky", "Linux", "test", "for", "now", "and", "push", "the", "PR."];
      var i = 0;
      voiceBars.classList.add("active");
      voiceTx.textContent = "Listening…";
      voiceSend.disabled = true;
      voiceTimer = setInterval(function () {
        i += 1;
        voiceTx.textContent = "“" + words.slice(0, i).join(" ") + (i < words.length ? "…" : "”");
        if (i >= words.length) {
          clearInterval(voiceTimer);
          voiceBars.classList.remove("active");
          voiceSend.disabled = false;
        }
      }, 150);
    });
  }
  if (voiceSend) {
    voiceSend.addEventListener("click", function () {
      voiceTx.textContent = "✓ Sent reply to Claude — stayed in Safari without switching windows.";
      voiceSend.disabled = true;
    });
  }

  // ═══════════════════════════════════════════════════════════════════
  // 07A · CROSS-PANE SPOTLIGHT WALKTHROUGH
  // ═══════════════════════════════════════════════════════════════════
  var spotT0 = document.getElementById("spot-target-0");
  var spotT1 = document.getElementById("spot-target-1");
  var spotPop = document.getElementById("spot-popover");
  var spotKick = document.getElementById("spot-kicker");
  var spotText = document.getElementById("spot-text");
  var spotNext = document.getElementById("spot-next");
  var spotDone = document.getElementById("spot-done");
  var spotStep = 0;

  function renderSpot() {
    if (!spotT0 || !spotT1) return;
    spotT0.classList.toggle("spot-lit", spotStep === 0);
    spotT1.classList.toggle("spot-lit", spotStep === 1);
    spotPop.style.display = spotStep === -1 ? "none" : "";
    if (spotStep === 0) {
      spotKick.textContent = "CLAUDE · STEP 1 OF 2";
      spotText.innerHTML = "Line 147 notifies windows while still holding the inbox mutex — moving <code>drop(guard)</code> above it fixes the hang.";
      spotNext.textContent = "Next →";
    } else if (spotStep === 1) {
      spotKick.textContent = "CLAUDE · STEP 2 OF 2";
      spotText.innerHTML = "The regression test lives in the <code>feat/chat-modals</code> worktree highlighted in your sidebar.";
      spotNext.textContent = "Restart";
    }
  }
  if (spotNext) {
    spotNext.addEventListener("click", function () {
      spotStep = spotStep === 0 ? 1 : 0;
      renderSpot();
    });
  }
  if (spotDone) {
    spotDone.addEventListener("click", function () {
      spotStep = -1;
      renderSpot();
      setTimeout(function () { spotStep = 0; renderSpot(); }, 1600);
    });
  }
  renderSpot();

  // ═══════════════════════════════════════════════════════════════════
  // 07B · MINI-DIFF APPROVAL SHEET
  // ═══════════════════════════════════════════════════════════════════
  var mdFoot = document.getElementById("md-foot");
  if (mdFoot) {
    mdFoot.querySelectorAll("[data-md]").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var act = btn.getAttribute("data-md");
        var msg =
          act === "approve" ? "✓ Approved — Codex applying migrations/014_modals.sql" :
          act === "edit" ? "Moved diff into Codex composer for edits" :
          "Rejected — Codex notified not to apply";
        var card = document.getElementById("minidiff-card");
        var prev = card.querySelector(".typed-feedback");
        if (prev) prev.remove();
        var fb = document.createElement("div");
        fb.className = "typed-feedback";
        fb.textContent = msg;
        card.appendChild(fb);
      });
    });
  }

  // ═══════════════════════════════════════════════════════════════════
  // 08 · SEMANTIC AUDIO SYNTH + EDGE-LIGHT PREVIEW
  // ═══════════════════════════════════════════════════════════════════
  var audioCtx = null;
  function getAudio() {
    var AC = window.AudioContext || window.webkitAudioContext;
    if (!AC) return null;
    if (!audioCtx) audioCtx = new AC();
    if (audioCtx.state === "suspended") audioCtx.resume();
    return audioCtx;
  }
  function tone(freq, start, dur, gainVal, type) {
    var ctx = getAudio();
    if (!ctx) return;
    var osc = ctx.createOscillator();
    var gain = ctx.createGain();
    osc.type = type || "sine";
    osc.frequency.setValueAtTime(freq, ctx.currentTime + start);
    gain.gain.setValueAtTime(0.0001, ctx.currentTime + start);
    gain.gain.exponentialRampToValueAtTime(gainVal, ctx.currentTime + start + 0.015);
    gain.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + start + dur);
    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.start(ctx.currentTime + start);
    osc.stop(ctx.currentTime + start + dur + 0.02);
  }

  var cueMeta = {
    yubikey: {
      tag: "Amber double-heartbeat",
      center: "🔑 Double-knock + amber double-pulse (hardware touch)",
      play: function () { tone(240, 0, 0.09, 0.18, "triangle"); tone(320, 0.14, 0.11, 0.2, "triangle"); },
    },
    decision: {
      tag: "Calm blue glow",
      center: "💬 Two-note chime + calm blue border (decision needed)",
      play: function () { tone(523.25, 0, 0.18, 0.12, "sine"); tone(659.25, 0.12, 0.24, 0.12, "sine"); },
    },
    secret: {
      tag: "Indigo vault shimmer",
      center: "🔒 Crystalline lock tone + indigo glow (secret requested)",
      play: function () { tone(784, 0, 0.14, 0.09, "sine"); tone(1046.5, 0.09, 0.22, 0.09, "sine"); },
    },
    countdown: {
      tag: "Pulsing coral edge",
      center: "⏱ Crisp tick + coral pulse (countdown active)",
      play: function () { tone(880, 0, 0.05, 0.11, "square"); tone(880, 0.35, 0.05, 0.11, "square"); },
    },
    done: {
      tag: "Emerald sweep",
      center: "✓ Warm major triad + emerald Edge Light sweep",
      play: function () {
        tone(523.25, 0, 0.28, 0.1, "sine");
        tone(659.25, 0.08, 0.28, 0.1, "sine");
        tone(783.99, 0.16, 0.36, 0.11, "sine");
      },
    },
  };

  var epWin = document.getElementById("edge-preview-win");
  var epTag = document.getElementById("ep-tag");
  var epCenter = document.getElementById("ep-center");

  document.querySelectorAll("[data-cue]").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var id = btn.getAttribute("data-cue");
      var info = cueMeta[id];
      if (!info || !epWin) return;
      epWin.className = "edge-preview-win cue-" + id;
      epTag.textContent = info.tag;
      epCenter.textContent = info.center;
      try { info.play(); } catch (_) {}
    });
  });
})();
