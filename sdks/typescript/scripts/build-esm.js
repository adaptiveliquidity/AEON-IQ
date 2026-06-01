const fs = require("fs");
const path = require("path");

const outDir = path.join(__dirname, "..", "dist");
const outFile = path.join(outDir, "index.mjs");

if (!fs.existsSync(outDir)) {
  throw new Error("dist directory does not exist. Run tsc first.");
}

const esmEntry = `import cjs from "./index.js";

export const MemoryClient = cjs.MemoryClient;
export default cjs;
`;

fs.writeFileSync(outFile, esmEntry, "utf8");
