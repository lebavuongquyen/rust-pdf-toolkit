const fs = require("fs");
const wasm = require("./wasm/pkg/node/pdffiller.js");

const template = fs.readFileSync("./reference/template.pdf");
const data = fs.readFileSync("./reference/data.json", "utf8");

const validation = JSON.parse(wasm.validate_pdf_result(template, data));
if (!validation.valid) {
  throw new Error(JSON.stringify(validation));
}

const result = JSON.parse(wasm.fill_pdf_result(template, data));
if (!result.success || result.filled < 9) {
  throw new Error(JSON.stringify(result));
}

const output = wasm.fill_pdf_bytes(template, data);
fs.writeFileSync("./output/wasm-node-filled.pdf", Buffer.from(output));

const base64 = wasm.fill_pdf_base64(template, data);
if (!base64 || base64.length < 1000) {
  throw new Error("Base64 output is empty");
}

console.log(JSON.stringify({
  validation,
  result,
  outputBytes: output.length,
  base64Length: base64.length,
  output: "output/wasm-node-filled.pdf"
}, null, 2));
