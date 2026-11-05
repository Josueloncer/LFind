// Se ejecuta antes del primer pintado. Solo lee valores cacheados por la propia app
// (los escribe App.tsx cuando las preferencias y la plataforma ya se conocen).
// La lista de temas debe coincidir con `src/lib/themes.ts`.
(function () {
  try {
    var r = document.documentElement;
    var known = ["dark", "light", "cyberpunk", "hacker", "minimal"];
    var t = localStorage.getItem("nova.theme") || "system";
    var dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    var resolved = known.indexOf(t) >= 0 ? t : dark ? "dark" : "light";
    r.setAttribute("data-theme", resolved);
    r.setAttribute("data-vibrancy", localStorage.getItem("nova.vibrancy") === "1" ? "on" : "off");
  } catch (e) {}
})();
