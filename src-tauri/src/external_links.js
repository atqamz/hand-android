(() => {
  const open = window.open;
  const external = ["http:", "https:", "mailto:", "tel:"];
  window.open = function (url, ...rest) {
    let target;
    try {
      target = new URL(String(url), window.location.href);
    } catch {
      return open.call(window, url, ...rest);
    }
    if (target.origin === window.location.origin || !external.includes(target.protocol)) {
      return open.call(window, url, ...rest);
    }
    window.__TAURI_INTERNALS__
      .invoke("plugin:opener|open_url", { url: target.href })
      .catch(() => {});
    return null;
  };
})();
