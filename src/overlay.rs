pub const OVERLAY_SCRIPT: &str = r#"
(function () {
  if (window.__focuslockInjected) {
    return;
  }
  window.__focuslockInjected = true;

  var bar = document.createElement('div');
  bar.id = '__focuslock_bar';
  bar.style.position = 'fixed';
  bar.style.top = '0';
  bar.style.left = '0';
  bar.style.right = '0';
  bar.style.height = '24px';
  bar.style.zIndex = '2147483647';
  bar.style.display = 'flex';
  bar.style.alignItems = 'center';
  bar.style.justifyContent = 'center';
  bar.style.background = 'rgba(17, 24, 39, 0.72)';
  bar.style.color = '#f8fafc';
  bar.style.fontFamily = '"Space Grotesk", "IBM Plex Sans", sans-serif';
  bar.style.fontSize = '12px';
  bar.style.letterSpacing = '0.08em';
  bar.style.textTransform = 'uppercase';
  bar.style.pointerEvents = 'none';
  bar.style.backdropFilter = 'blur(6px)';
  bar.style.position = 'fixed';
  bar.style.overflow = 'hidden';

  var text = document.createElement('div');
  text.id = '__focuslock_text';
  text.textContent = 'Focuslock';
  bar.appendChild(text);

  var prompt = document.createElement('div');
  prompt.id = '__focuslock_prompt';
  prompt.style.position = 'absolute';
  prompt.style.top = '0';
  prompt.style.left = '0';
  prompt.style.right = '0';
  prompt.style.height = '24px';
  prompt.style.display = 'none';
  prompt.style.alignItems = 'center';
  prompt.style.justifyContent = 'center';
  prompt.style.gap = '10px';
  prompt.style.background = 'rgba(15, 23, 42, 0.92)';
  prompt.style.color = '#f8fafc';
  prompt.style.fontSize = '12px';
  prompt.style.letterSpacing = '0.06em';
  prompt.style.textTransform = 'uppercase';
  prompt.style.pointerEvents = 'auto';

  var promptLabel = document.createElement('span');
  promptLabel.id = '__focuslock_prompt_label';
  promptLabel.textContent = 'Enter unlock PIN';

  var promptInput = document.createElement('span');
  promptInput.id = '__focuslock_prompt_input';
  promptInput.textContent = '';
  promptInput.style.fontFamily = '"IBM Plex Mono", "JetBrains Mono", monospace';
  promptInput.style.fontSize = '12px';
  promptInput.style.letterSpacing = '0.2em';

  prompt.appendChild(promptLabel);
  prompt.appendChild(promptInput);
  bar.appendChild(prompt);

  var root = document.body || document.documentElement;
  root.appendChild(bar);

  var promptActive = false;
  var promptBuffer = '';

  window.__focuslockSetTimer = function (value) {
    var node = document.getElementById('__focuslock_text');
    if (node) {
      node.textContent = value;
    }
  };

  function updatePrompt() {
    var label = document.getElementById('__focuslock_prompt_label');
    var input = document.getElementById('__focuslock_prompt_input');
    if (input) {
      input.textContent = promptBuffer.replace(/./g, '•');
    }
  }

  window.__focuslockShowPrompt = function () {
    promptActive = true;
    promptBuffer = '';
    var label = document.getElementById('__focuslock_prompt_label');
    if (label) {
      label.textContent = 'Enter unlock PIN';
    }
    var node = document.getElementById('__focuslock_prompt');
    if (node) {
      node.style.display = 'flex';
    }
    updatePrompt();
  };

  window.__focuslockHidePrompt = function () {
    promptActive = false;
    var node = document.getElementById('__focuslock_prompt');
    if (node) {
      node.style.display = 'none';
    }
    promptBuffer = '';
  };

  window.__focuslockSetPromptMessage = function (message) {
    var label = document.getElementById('__focuslock_prompt_label');
    if (label) {
      label.textContent = message;
    }
  };

  window.__focuslockClearPrompt = function () {
    promptBuffer = '';
    updatePrompt();
  };

  window.addEventListener('keydown', function (event) {
    if (!promptActive && event.ctrlKey && event.shiftKey && (event.key === 'q' || event.key === 'Q')) {
      if (window.ipc && window.ipc.postMessage) {
        window.ipc.postMessage('escape_open');
      }
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    if (!promptActive) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();

    if (event.key === 'Escape') {
      if (window.ipc && window.ipc.postMessage) {
        window.ipc.postMessage('escape_cancel');
      }
      return;
    }
    if (event.key === 'Enter') {
      if (window.ipc && window.ipc.postMessage) {
        window.ipc.postMessage('escape_submit:' + promptBuffer);
      }
      return;
    }
    if (event.key === 'Backspace') {
      promptBuffer = promptBuffer.slice(0, -1);
      updatePrompt();
      return;
    }
    if (event.key && event.key.length === 1) {
      promptBuffer += event.key;
      updatePrompt();
    }
  }, true);
})();
"#;

pub fn set_timer_script(value: &str) -> String {
    format!("window.__focuslockSetTimer && window.__focuslockSetTimer({value:?});")
}

pub const SHOW_PROMPT_SCRIPT: &str =
    "window.__focuslockShowPrompt && window.__focuslockShowPrompt();";
pub const HIDE_PROMPT_SCRIPT: &str =
    "window.__focuslockHidePrompt && window.__focuslockHidePrompt();";
pub const CLEAR_PROMPT_SCRIPT: &str =
    "window.__focuslockClearPrompt && window.__focuslockClearPrompt();";

pub fn set_prompt_message_script(message: &str) -> String {
    format!(
        "window.__focuslockSetPromptMessage && window.__focuslockSetPromptMessage({message:?});"
    )
}
