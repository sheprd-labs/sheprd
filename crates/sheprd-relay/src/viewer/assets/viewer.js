(() => {
  let fragment = location.hash.slice(1);
  const landing = location.origin + location.pathname;
  history.replaceState(null, "", location.pathname);
  const states = {
    join: ["Invitation", "Join this session?", "Follow the conversation as it happens."],
    connecting: ["Connecting", "Connecting to the host", "Opening an encrypted connection."],
    reconnecting: ["Reconnecting", "Reconnecting to the host", "Waiting for the shared session."],
    live: ["Live", "", ""],
    offline: ["Unavailable", "Host offline or link revoked", "Ask the owner to check the host and invitation."],
    withdrawn: ["Unavailable", "Access ended", "The owner may have revoked the link or gone offline."],
    expired: ["Expired", "This invitation has expired", "Ask the owner for a new link."],
    ended: ["Ended", "Session ended", "The shared conversation has finished."],
    invalid: ["Unavailable", "Invitation unavailable", "Open the complete invitation link from the owner."],
    version: ["Unsupported", "Unsupported version", "This relay needs a viewer compatible with the invitation."]
  };
  // A deliberately small Markdown subset. The DOM only ever receives text nodes.
  function markdown(container, text) {
    const blocks = text.split(/^```[^\n]*$/m);
    blocks.forEach((block, index) => {
      const el = document.createElement(index % 2 ? "pre" : "p");
      if (index % 2) el.textContent = block;
      else {
        for (const part of block.split(/(\*\*[^*\n]+\*\*|`[^`\n]+`)/g)) {
          if (part.startsWith("**") && part.endsWith("**") && part.length > 4) {
            el.appendChild(document.createElement("strong")).textContent = part.slice(2,-2);
          } else if (part.startsWith("`") && part.endsWith("`") && part.length > 2) {
            el.appendChild(document.createElement("code")).textContent = part.slice(1,-1);
          } else el.append(part);
        }
      }
      container.append(el);
    });
  }
  addEventListener("DOMContentLoaded", () => {
    const byId = id => document.getElementById(id);
    let worker, timer;
    function stopWorker() {
      if (worker) {
        worker.onmessage = worker.onerror = null;
        worker.terminate();
        worker = null;
      }
    }
    function state(name) {
      document.body.dataset.state = name;
      [byId("badge").textContent, byId("title").textContent, byId("detail").textContent] = states[name];
      byId("notice").hidden = name === "live";
      byId("scope").hidden = name !== "live";
      byId("history").hidden = true;
      byId("connection").textContent = name === "live" ? "Encrypted" :
        name === "join" ? "Waiting to join" : states[name][0];
      byId("join").hidden = name !== "join";
      byId("conversation").replaceChildren();
      if (!["join","connecting","live","reconnecting"].includes(name)) {
        stopWorker();
        clearTimeout(timer);
      }
    }
    function render(transcript) {
      if (transcript.truncated || !transcript.entries.length) {
        byId("history").hidden = false;
        byId("history").textContent = transcript.entries.length ?
          "Showing recent shared messages." : "Waiting for shared messages.";
      }
      for (const {kind} of transcript.entries) {
        if (!["user","agent","tool"].includes(kind.type)) continue;
        const article = document.createElement("article");
        article.dataset.role = kind.type;
        article.appendChild(document.createElement("h2")).textContent =
          {user:"User", agent:transcript.agent || "Agent", tool:"Tool"}[kind.type];
        markdown(article, kind.type === "tool" ?
          [kind.title,...kind.content.filter(x => x.type === "text").map(x => x.text)].join("\n") : kind.text);
        byId("conversation").append(article);
      }
    }
    byId("join").onclick = () => {
      state("connecting");
      worker = new Worker("worker.js", {type:"module"});
      worker.onerror = () => state("invalid");
      worker.onmessage = ({data}) => {
        if (data.expiry) {
          byId("expiry").textContent = new Date(data.expiry).toLocaleString();
          clearTimeout(timer);
          timer = setTimeout(() => state("expired"), Math.min(2147483647, Math.max(0,data.expiry-Date.now())));
        }
        if (data.state) state(data.state);
        if (data.transcript) {
          render(data.transcript);
          worker.postMessage("rendered");
        }
      };
      worker.postMessage({landing, fragment});
      fragment = "";
    };
    function invitation() {
      stopWorker();
      clearTimeout(timer);
      byId("expiry").textContent = "Available after joining";
      state(fragment && fragment.length + landing.length < 8192 ? "join" : "invalid");
    }
    invitation();
    addEventListener("hashchange", () => {
      fragment = location.hash.slice(1);
      history.replaceState(null, "", location.pathname);
      invitation();
    });
    document.body.hidden = false;
    addEventListener("pagehide", () => { fragment = ""; state("invalid"); });
  });
})();
