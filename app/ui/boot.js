"use strict";
/* ───────── boot ───────── */
if (MODE === "locked") { lockedScreen(); }
else { render(); load(); setInterval(() => { if (!document.hidden) load() }, 3000); }
document.addEventListener("visibilitychange", () => !document.hidden && load());
