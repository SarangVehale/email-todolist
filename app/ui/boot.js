"use strict";
/* ───────── boot ───────── */
render(); load(); setInterval(load, 3000);
document.addEventListener("visibilitychange", () => !document.hidden && load());
