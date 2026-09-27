(function () {
  "use strict";

  var VERDICTS = [
    { key: "all", label: "All steps" },
    { key: "change", label: "Changes" },
    { key: "inspect", label: "Inspects" },
    { key: "keep", label: "Advises only" },
    { key: "evidence", label: "Needs capture" },
    { key: "skip", label: "Not for this GPU" }
  ];
  var PHASES = { 1: "Normal boot", 2: "Safe Mode", 3: "Normal boot, same user" };

  function el(tag, className, text) {
    var node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function verdictLabel(key) {
    for (var i = 0; i < VERDICTS.length; i++) {
      if (VERDICTS[i].key === key) return VERDICTS[i].label;
    }
    return key;
  }

  function option(group, name, value, label, count, checked) {
    var wrap = el("div", "option");
    var id = name + "-" + value;
    var input = el("input");
    input.type = "radio";
    input.name = name;
    input.value = value;
    input.id = id;
    input.checked = checked;
    var text = el("label");
    text.htmlFor = id;
    text.appendChild(document.createTextNode(label));
    if (count !== null) {
      var badge = el("span", "option__count", String(count));
      text.appendChild(badge);
    }
    wrap.appendChild(input);
    wrap.appendChild(text);
    group.appendChild(wrap);
    return input;
  }

  function initRegister() {
    var root = document.querySelector("[data-register]");
    var data = window.FRAMETIME_REGISTER;
    if (!root || !data) return;

    var branchBox = root.querySelector("[data-branches]");
    var verdictBox = root.querySelector("[data-verdicts]");
    var summary = root.querySelector("[data-summary]");
    var list = root.querySelector("[data-list]");
    var source = root.querySelector("[data-source]");
    var state = { branch: data.branches[0].key, verdict: "all" };

    data.branches.forEach(function (branch, index) {
      option(branchBox, "gpu", branch.key, branch.label, null, index === 0);
    });

    function counts(branch) {
      var tally = { all: 0 };
      data.steps.forEach(function (step) {
        var v = branch.results[step.id].v;
        tally[v] = (tally[v] || 0) + 1;
        tally.all += 1;
      });
      return tally;
    }

    function currentBranch() {
      for (var i = 0; i < data.branches.length; i++) {
        if (data.branches[i].key === state.branch) return data.branches[i];
      }
      return data.branches[0];
    }

    function renderVerdicts(tally) {
      verdictBox.textContent = "";
      VERDICTS.forEach(function (verdict) {
        var count = tally[verdict.key] || 0;
        var input = option(verdictBox, "verdict", verdict.key, verdict.label, count, state.verdict === verdict.key);
        if (count === 0 && verdict.key !== "all") input.disabled = true;
      });
    }

    function renderSummary(branch, tally, shown) {
      summary.textContent = "";
      var head = el("strong", "", branch.label + ".");
      summary.appendChild(head);
      var parts = VERDICTS.filter(function (v) { return v.key !== "all" && tally[v.key]; }).map(function (v) {
        return v.label + " " + tally[v.key];
      });
      summary.appendChild(document.createTextNode(" " + parts.join(" · ") + ". Showing " + shown + " of " + tally.all + " steps."));
    }

    function stepRow(step, result) {
      var item = el("li");
      var details = el("details", "step step--" + result.v);
      var head = el("summary");
      head.appendChild(el("span", "step__code", step.id));
      var title = el("span", "step__title", step.title);
      title.appendChild(el("span", "step__area", step.category));
      head.appendChild(title);
      var meta = el("span", "step__meta", step.risk === "Safe" ? "" : step.risk + " risk");
      meta.setAttribute("data-risk", step.risk);
      head.appendChild(meta);
      var verdictWrap = el("span", "step__verdict");
      verdictWrap.appendChild(el("span", "verdict verdict--" + result.v, verdictLabel(result.v)));
      head.appendChild(verdictWrap);
      details.appendChild(head);

      var body = el("div", "step__body");
      body.appendChild(el("p", "step__line", result.line));
      var facts = [step.category, step.risk + " risk", step.reboot ? "needs a reboot" : "no reboot"];
      if (step.check) facts.push("check only");
      body.appendChild(el("p", "step__facts", facts.join(" · ")));
      details.appendChild(body);
      item.appendChild(details);
      return item;
    }

    function renderList(branch) {
      list.textContent = "";
      var shown = 0;
      [1, 2, 3].forEach(function (phase) {
        var inPhase = data.steps.filter(function (s) { return s.id.indexOf("P" + phase + ":") === 0; });
        var visible = inPhase.filter(function (s) {
          return state.verdict === "all" || branch.results[s.id].v === state.verdict;
        });
        if (!visible.length) return;
        shown += visible.length;
        var section = el("section", "phase");
        section.setAttribute("aria-label", "Phase " + phase + ", " + PHASES[phase]);
        var head = el("div", "phase__head");
        var title = el("h3", "phase__title");
        title.appendChild(el("span", "mono", "Phase " + phase));
        title.appendChild(document.createTextNode(PHASES[phase]));
        head.appendChild(title);
        head.appendChild(el("p", "phase__meta", visible.length + " of " + inPhase.length + " steps"));
        section.appendChild(head);
        var ol = el("ol", "steps");
        visible.forEach(function (step) { ol.appendChild(stepRow(step, branch.results[step.id])); });
        section.appendChild(ol);
        list.appendChild(section);
      });
      if (!shown) {
        list.appendChild(el("p", "register__empty", "No step on this branch has that verdict. Choose All steps."));
      }
      return shown;
    }

    function render() {
      var branch = currentBranch();
      var tally = counts(branch);
      renderVerdicts(tally);
      var shown = renderList(branch);
      renderSummary(branch, tally, shown);
    }

    branchBox.addEventListener("change", function (event) {
      state.branch = event.target.value;
      render();
    });
    verdictBox.addEventListener("change", function (event) {
      state.verdict = event.target.value;
      render();
      var focused = verdictBox.querySelector('input[value="' + state.verdict + '"]');
      if (focused) focused.focus();
    });

    source.textContent = "Captured from the source build's " + data.command.replace(/^frametime /, "") +
      " output on " + data.captured + ". A preview changes nothing; the verdict describes what a live run from an authenticated package would do.";
    root.hidden = false;
    render();
  }

  function initLightbox() {
    var dialog = document.getElementById("lightbox");
    if (!dialog || typeof dialog.showModal !== "function") return;
    var img = dialog.querySelector("img");
    var caption = dialog.querySelector("[data-caption]");
    document.querySelectorAll(".plate__zoom[data-full]").forEach(function (button) {
      button.addEventListener("click", function (event) {
        event.preventDefault();
        var thumb = button.querySelector("img");
        var title = button.parentNode.querySelector("figcaption strong");
        img.src = button.getAttribute("data-full");
        img.alt = thumb ? thumb.alt : "";
        caption.textContent = title ? title.textContent : "";
        dialog.showModal();
      });
    });
    dialog.addEventListener("click", function (event) {
      if (event.target === dialog) dialog.close();
    });
  }

  function initCopy() {
    document.querySelectorAll("[data-copy]").forEach(function (button) {
      var target = document.getElementById(button.getAttribute("data-copy"));
      if (!target) return;
      var reset;
      button.addEventListener("click", function () {
        var text = target.textContent;
        function done(label) {
          button.textContent = label;
          clearTimeout(reset);
          reset = setTimeout(function () { button.textContent = "Copy"; }, 2000);
        }
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(text).then(function () { done("Copied"); }, function () { select(target); done("Selected"); });
        } else {
          select(target);
          done("Selected");
        }
      });
    });
  }

  function select(node) {
    var range = document.createRange();
    range.selectNodeContents(node);
    var selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
  }

  initRegister();
  initLightbox();
  initCopy();
})();
