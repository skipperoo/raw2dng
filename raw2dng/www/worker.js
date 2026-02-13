import init, { convert_raw_to_dng } from "./pkg/raw_dng_converter.js";

let wasmInitialized = false;
let opfsRoot = null;

onmessage = async (e) => {
  const { type, payload } = e.data;

  if (type === "init") {
    try {
      await init();
      wasmInitialized = true;
      opfsRoot = await navigator.storage.getDirectory();
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

    const { file, id } = payload;
    const fileName = file.name;
    try {
      // 1. Save RAW to OPFS (Swap)
      const rawFileHandle = await opfsRoot.getFileHandle(fileName, { create: true });
      if (rawFileHandle.createWritable) {
        const writable = await rawFileHandle.createWritable();
        await writable.write(file);
        await writable.close();
      } else if (rawFileHandle.createSyncAccessHandle) {
        const accessHandle = await rawFileHandle.createSyncAccessHandle();
        const buffer = await file.arrayBuffer();
        accessHandle.truncate(0);
        accessHandle.write(new Uint8Array(buffer));
        accessHandle.flush();
        accessHandle.close();
      }

      // 2. Read back for conversion (to ensure we're using the "swapped" version and free memory)
      // Actually, we already have 'file' which is a pointer. 
      // To truly save memory, we should ensure the 'file' object is not held elsewhere.
      const rawBuffer = await file.arrayBuffer();
      const uint8Array = new Uint8Array(rawBuffer);

      // 3. Convert
      const dngData = convert_raw_to_dng(uint8Array, fileName);
      
      // 4. Save DNG to OPFS
      const dngFileName = fileName.replace(/\.[^/.]+$/, "") + ".dng";
      const dngFileHandle = await opfsRoot.getFileHandle(dngFileName, { create: true });
      
      if (dngFileHandle.createWritable) {
        const writable = await dngFileHandle.createWritable();
        await writable.write(dngData);
        await writable.close();
      } else if (dngFileHandle.createSyncAccessHandle) {
        const accessHandle = await dngFileHandle.createSyncAccessHandle();
        accessHandle.truncate(0);
        accessHandle.write(dngData);
        accessHandle.flush();
        accessHandle.close();
      }

      // 5. Cleanup RAW from memory (explicitly)
      // uint8Array is now out of scope after this block

      postMessage({
        type: "convert-success",
        payload: {
          originalName: fileName,
          dngName: dngFileName,
          id,
          size: dngData.byteLength
        },
      });
    } catch (err) {
      postMessage({ type: "convert-error", payload: { error: err.toString(), id } });
    }
  }
};
