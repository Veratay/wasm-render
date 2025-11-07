const webpack = require("webpack");
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");
const HtmlWebpackPlugin = require("html-webpack-plugin");
const path = require("path");

module.exports = (env, args) => {
  const isProductionMode = args.mode === "production";
  const entry = {
    app: "./index.js",
    tests: "./tests/browser/tests.js",
  };

  return {
    entry,
    output: {
      path: path.resolve(__dirname, "dist"),
      filename: isProductionMode
        ? "[name].[contenthash].js"
        : "[name].[hash].js",
    },
    plugins: [
      new HtmlWebpackPlugin({
        template: "index.html",
        filename: "index.html",
        chunks: ["app"],
      }),
      new HtmlWebpackPlugin({
        template: "tests/browser/index.html",
        filename: "tests/index.html",
        chunks: ["tests"],
      }),
      new WasmPackPlugin({
        crateDirectory: path.resolve(__dirname, "."),
        outName: "render",
      }),
      new webpack.ProvidePlugin({
        TextDecoder: ["text-encoding", "TextDecoder"],
        TextEncoder: ["text-encoding", "TextEncoder"],
      }),
    ],
    experiments: {
      asyncWebAssembly: true, // 👈 enables wasm loading
    },
    module: {
      rules: [
        {
          test: /\.wasm$/,
          type: "webassembly/async", // 👈 mark wasm modules properly
        },
      ],
    },
  };
};
