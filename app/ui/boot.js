"use strict";
/* ───────── boot ───────── */
if (MODE === "locked") { lockedScreen(); }
else { render(); load(); setInterval(load, 3000); }
document.addEventListener("visibilitychange", () => !document.hidden && load());
