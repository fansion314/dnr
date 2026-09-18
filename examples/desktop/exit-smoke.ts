// Opens a window, then exits with status 7 (Deno) or 9 (Node).
// Use "error" to check an uncaught GUI callback error (status 1).
import process from "node:process";

const watchdog = setTimeout(() => {
  console.error("DNR_GUI_EXIT_TIMEOUT");
  Deno.exit(1);
}, 20000);
const win = new Deno.BrowserWindow({ title: "dnr exit verification", width: 400, height: 200 });
win.addEventListener("load", async () => {
  const location = await win.executeJs("location.href");
  if (!location.ok || location.value === "about:blank") return;
  clearTimeout(watchdog);
  console.log("DNR_GUI_EXIT_READY");
  // Leave the window open: the host must close it and clean up the backend.
  if (Deno.args[0] === "node") process.exit(9);
  if (Deno.args[0] === "error") throw new Error("DNR_EXPECTED_GUI_ERROR");
  Deno.exit(7);
});
win.navigate("data:text/html,<title>dnr exit</title><h1>Explicit exit check</h1>");
