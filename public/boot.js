// Se ejecuta antes del primer pintado. Solo lee valores cacheados por la propia app
// (los escribe App.tsx cuando las preferencias y la plataforma ya se conocen).
(function () {
  try {
    var r = document.documentElement;
    var t = localStorage.getItem("nova.theme") || "system";
    var dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    r.setAttribute("data-theme", t === "system" ? (dark ? "dark" : "light") : t);
    r.setAttribute("data-vibrancy", localStorage.getItem("nova.vibrancy") === "1" ? "on" : "off");
  } catch (e) {}
})();
