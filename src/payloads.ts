import {
  getDefaultProcessSharedBufferPrimitives,
  ProcessSharedBuffer,
} from "knitting/process-shared-buffer";

const BYTE_FILL_VALUES = [0xAB, 0xBC, 0xCD, 0xDE] as const;

export const makeBytePayloads = (bytes: number): Uint8Array[] =>
  BYTE_FILL_VALUES.map((fill) => new Uint8Array(bytes).fill(fill));

export const makePsbPayloads = (bytes: number): ProcessSharedBuffer[] => {
  const primitives = getDefaultProcessSharedBufferPrimitives();
  return BYTE_FILL_VALUES.map((fill) => {
    const psb = ProcessSharedBuffer.create(bytes, primitives);
    psb.bytes().fill(fill);
    return psb;
  });
};

export const powerOfTwoBytes = (minBytes: number, maxBytes: number): number[] => {
  const sizes: number[] = [];
  for (let bytes = minBytes; bytes <= maxBytes; bytes *= 2) sizes.push(bytes);
  return sizes;
};
