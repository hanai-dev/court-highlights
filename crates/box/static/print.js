// Court signs: the Print button opens the browser's print dialog (one sign per page via the print stylesheet).
(function () {
  'use strict';
  var button = document.querySelector('[data-print]');
  if (button) button.addEventListener('click', function () { window.print(); });
})();
