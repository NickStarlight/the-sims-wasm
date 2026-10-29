/* tslint:disable */
/* eslint-disable */

export class Artifact {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    metadataJson(): string;
    /**
     * Serialize only the fields needed to instantiate and link the guest module.
     */
    runtimeMetadataJson(): string;
    /**
     * Copies the emitted module bytes to the host for WebAssembly.compile.
     */
    wasm(): Uint8Array;
}

export class Compiler {
    free(): void;
    [Symbol.dispose](): void;
    addModule(bytes: Uint8Array, base: number): void;
    compile(roots: Uint32Array): Artifact;
    loadBytes(): Uint8Array;
    loadPlanJson(): string;
    constructor(executable: Uint8Array);
}

/**
 * Metadata used to verify and compile the user's local installation.
 */
export function completeProfileJson(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_artifact_free: (a: number, b: number) => void;
    readonly __wbg_compiler_free: (a: number, b: number) => void;
    readonly artifact_metadataJson: (a: number) => [number, number, number, number];
    readonly artifact_runtimeMetadataJson: (a: number) => [number, number, number, number];
    readonly artifact_wasm: (a: number) => [number, number];
    readonly compiler_addModule: (a: number, b: number, c: number, d: number) => [number, number];
    readonly compiler_compile: (a: number, b: number, c: number) => [number, number, number];
    readonly compiler_loadBytes: (a: number) => [number, number];
    readonly compiler_loadPlanJson: (a: number) => [number, number, number, number];
    readonly compiler_new: (a: number, b: number) => [number, number, number];
    readonly completeProfileJson: () => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
