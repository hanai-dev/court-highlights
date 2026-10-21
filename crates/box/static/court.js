// Court page: Clip that without a page reload, the saving ring (FR-8), and the clip list refreshing while a clip saves.
// The page works without this script: the form posts and the box redirects back.
(function () {
  'use strict';
  var form = document.querySelector('[data-tap]');
  var saving = document.querySelector('[data-saving]');
  var savingText = document.querySelector('[data-saving-text]');
  var ring = document.querySelector('[data-ring]');
  var tapError = document.querySelector('[data-tap-error]');
  var clips = document.querySelector('[data-clips]');
  if (!form || !clips) return;

  var countdown = null;
  var poller = null;

  function refreshClips() {
    fetch(clips.getAttribute('data-clips-url'), { headers: { Accept: 'text/html' }, cache: 'no-store' })
      .then(function (r) { return r.ok ? r.text() : null; })
      .then(function (html) {
        if (html === null) return;
        clips.innerHTML = html; // server-rendered and escaped by the box
        if (!clips.querySelector('[data-clip-state="saving"]') && !countdown) stopPolling();
      })
      .catch(function () { /* the next poll retries */ });
  }

  function startPolling() {
    if (!poller) poller = setInterval(refreshClips, 2000);
  }

  function stopPolling() {
    clearInterval(poller);
    poller = null;
  }

  function showSaving(readyInMs) {
    var endsAt = Date.now() + readyInMs;
    saving.hidden = false;
    // Restart the fill: the ring measures the time left in this clip's window (up to 15 s, linear).
    ring.classList.remove('filling');
    void ring.getBoundingClientRect();
    ring.style.animationDuration = Math.max(readyInMs, 1) + 'ms';
    ring.classList.add('filling');
    clearInterval(countdown);
    function tick() {
      var left = Math.ceil((endsAt - Date.now()) / 1000);
      if (left > 0) {
        savingText.textContent = 'Saving… ready in ' + left + ' s';
      } else {
        savingText.textContent = 'Saving… making your clip';
        clearInterval(countdown);
        countdown = null;
      }
    }
    tick();
    countdown = setInterval(tick, 1000);
  }

  form.addEventListener('submit', function (event) {
    event.preventDefault();
    tapError.hidden = true;
    fetch(form.action, { method: 'POST', headers: { Accept: 'application/json' } })
      .then(function (r) { return r.json().then(function (body) { return { ok: r.ok, body: body }; }); })
      .then(function (res) {
        if (!res.ok) {
          tapError.textContent = res.body.error || 'No clip was saved. Try again.';
          tapError.hidden = false;
          return;
        }
        if (navigator.vibrate) navigator.vibrate(150);
        showSaving(res.body.ready_in_ms);
        refreshClips();
        startPolling();
      })
      .catch(function () {
        tapError.textContent = "The court box didn't answer. Check you're on the venue Wi-Fi and tap again.";
        tapError.hidden = false;
      });
  });

  if (clips.querySelector('[data-clip-state="saving"]')) startPolling();
})();
