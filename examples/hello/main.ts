import { readFileSync, readdirSync } from "node:fs";

const message: string = Deno.readTextFileSync(new URL("./message.txt", import.meta.url));
console.log(message.trim());
console.log(JSON.stringify({ args: Deno.args, cwd: Deno.cwd(), main: import.meta.url }));
if (readFileSync(new URL("./message.txt", import.meta.url), "utf8") !== message) {
  throw new Error("Deno and Node filesystem views disagree");
}
console.log("files", readdirSync(new URL("./", import.meta.url)).sort());
