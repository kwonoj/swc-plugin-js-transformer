const { transformSync } = require("@swc/core");
const path = require("path");

let output = transformSync(`console.log("hello")`, {
  jsc: {
    experimental: {
      plugins: [
        [
          path.resolve(__dirname, "target/wasm32-wasi/release/swc_plugin_js_transformer.wasm"),
          {
            transformImplPath: "visitor.js"
          }]
      ]
    }
  }
});

console.log(output.code);