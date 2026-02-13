import init, { convert_raw_to_dng } from "./pkg/raw_dng_converter.js";

let wasmInitialized = false;

onmessage = async (e) => {
  const { type, payload } = e.data;

  if (type === "init") {
    try {
      await init();
      wasmInitialized = true;
      postMessage({ type: "init-success" });
    } catch (err) {
      postMessage({ type: "init-error", payload: err.toString() });
    }
    return;
  }

  if (type === "convert") {
    if (!wasmInitialized) {
      postMessage({ type: "error", payload: "Wasm not initialized" });
      return;
    }

    const { buffer, fileName, id } = payload;
    try {
      const uint8Array = new Uint8Array(buffer);
      const dngData = convert_raw_to_dng(uint8Array, fileName);
      // dngData is a Uint8Array. We send its buffer back as transferable.
      postMessage(
        {
          type: "convert-success",
          payload: {
            dngBuffer: dngData.buffer,
            fileName,
            id,
          },
        },
        [dngData.buffer]
      );
    } catch (err) {
      postMessage({ type: "convert-error", payload: { error: err.toString(), id } });
    }
  }
};
