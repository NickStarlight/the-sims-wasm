/* tslint:disable */
/* eslint-disable */

/**
 * A completed RGBA frame copied out once by the browser presentation host.
 */
export class Frame {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    pixels(): Uint8Array;
    readonly height: number;
    readonly width: number;
}

/**
 * Retains the exact completed surface until its packed bytes are copied to the host.
 */
export class PackedFrame {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    pixels(): Uint8Array;
    readonly format: number;
    readonly height: number;
    readonly pitch: number;
    readonly width: number;
}

/**
 * One explicit process per guest memory. free() drops metadata, not the caller's memory.
 */
export class Process {
    free(): void;
    [Symbol.dispose](): void;
    addFileReader(path: string, size: number, reader: Function): void;
    addFile(path: string, bytes: Uint8Array): void;
    adopt(address: number, size: number, writable: boolean): void;
    advanceTime(now: number): void;
    beginCall(id: number, args: Uint32Array): string;
    beginException(id: number, args: Uint32Array, registers: Uint32Array): string;
    /**
     * A DWORD fits exactly in f64, leaving -1 for transport failure. Returning
     * one scalar avoids allocating a JS array for the Result multi-value ABI.
     * The synchronous caller consumes an error before another guest can run.
     */
    callSmallForThreadStatus(thread_id: number, id: number, count: number, first: number, second: number, third: number, fourth: number, now_ms?: number | null, wall_ms?: number | null): number;
    /**
     * Select the calling thread and invoke the scalar API in one boundary crossing.
     * Selection precedes validation, just as `setThread` followed by `callSmall` does.
     */
    callSmallForThread(thread_id: number, id: number, count: number, first: number, second: number, third: number, fourth: number, now_ms?: number | null, wall_ms?: number | null): number;
    /**
     * Common synchronous calls cross the JS/Wasm boundary with scalar
     * arguments, avoiding a temporary allocation for wasm-bindgen's slice.
     * Keep the same arity, clock, memory and API validation as `call`.
     */
    callSmall(id: number, count: number, first: number, second: number, third: number, fourth: number, now_ms?: number | null, wall_ms?: number | null): number;
    call(id: number, args: Uint32Array, now_ms?: number | null, wall_ms?: number | null): number;
    cancelCall(token: number): boolean;
    cancelException(token: number): boolean;
    cancelWait(token: number): boolean;
    cpuHelper(id: number, registers: Int32Array, monotonic_ms: number): Int32Array;
    exportAddress(dll: string, symbol: any): number;
    finishThread(id: number, code: number): void;
    hasReadyWaits(): boolean;
    heapAlloc(size: number): number;
    heapFree(address: number): void;
    heapRealloc(address: number, size: number): number;
    heapSize(address: number): number;
    importRegistry(json: string): void;
    imports(): string;
    installModules(configuration: string): void;
    keyInput(key: number, down: boolean, repeat: boolean): void;
    map(address: number, bytes: Uint8Array, writable: boolean): void;
    mixAudio(frames: number, rate: number, now_ms: number): StereoBlock;
    mouseInput(x: number, y: number, buttons: number): void;
    constructor(memory: WebAssembly.Memory, allocation_start: number, limit: number);
    nextMediaTimer(now_ms: number): string | undefined;
    read(address: number, size: number): Uint8Array;
    reserve(address: number, size: number): void;
    /**
     * Resolve a supported Windows import to a JSON service descriptor.
     */
    resolve(dll: string, name: string): string | undefined;
    restoreJournal(bytes: Uint8Array): number;
    resumeCall(token: number, result: number): string;
    resumeException(token: number, registers: Uint32Array): string;
    /**
     * Preserve structured wait/exit outcomes, but return immediate values
     * directly so uncontended locks need neither a slice copy nor JSON.
     */
    scheduleCallSmall(id: number, count: number, first: number, second: number, third: number, fourth: number, now: number): any;
    scheduleCall(id: number, args: Uint32Array, now: number): string;
    setCommandLine(address: number): void;
    setThread(id: number): void;
    takeFileChanges(): Uint8Array;
    takeFrame(): Frame | undefined;
    takePackedFrame(): PackedFrame | undefined;
    takeReadyWaits(): string;
    takeRegistryChanges(): string | undefined;
    takeRunnableThreads(): string;
    takeSyncCallError(): any;
    virtualAlloc(address: number, size: number): number;
    virtualFree(address: number): void;
    write(address: number, bytes: Uint8Array): void;
}

/**
 * Owned PCM block: only stereo sample arrays cross the browser boundary.
 */
export class StereoBlock {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    left(): Float32Array;
    right(): Float32Array;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_frame_free: (a: number, b: number) => void;
    readonly __wbg_packedframe_free: (a: number, b: number) => void;
    readonly __wbg_process_free: (a: number, b: number) => void;
    readonly __wbg_stereoblock_free: (a: number, b: number) => void;
    readonly frame_height: (a: number) => number;
    readonly frame_pixels: (a: number) => any;
    readonly frame_width: (a: number) => number;
    readonly packedframe_format: (a: number) => number;
    readonly packedframe_height: (a: number) => number;
    readonly packedframe_pitch: (a: number) => number;
    readonly packedframe_pixels: (a: number) => any;
    readonly packedframe_width: (a: number) => number;
    readonly process_addFile: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly process_addFileReader: (a: number, b: number, c: number, d: number, e: any) => [number, number];
    readonly process_adopt: (a: number, b: number, c: number, d: number) => [number, number];
    readonly process_advanceTime: (a: number, b: number) => [number, number];
    readonly process_beginCall: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly process_beginException: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly process_call: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number];
    readonly process_callSmall: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => [number, number, number];
    readonly process_callSmallForThread: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number) => [number, number, number];
    readonly process_callSmallForThreadStatus: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number) => number;
    readonly process_cancelCall: (a: number, b: number) => number;
    readonly process_cancelException: (a: number, b: number) => number;
    readonly process_cancelWait: (a: number, b: number) => number;
    readonly process_cpuHelper: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly process_exportAddress: (a: number, b: number, c: number, d: any) => number;
    readonly process_finishThread: (a: number, b: number, c: number) => [number, number];
    readonly process_hasReadyWaits: (a: number) => number;
    readonly process_heapAlloc: (a: number, b: number) => [number, number, number];
    readonly process_heapFree: (a: number, b: number) => [number, number];
    readonly process_heapRealloc: (a: number, b: number, c: number) => [number, number, number];
    readonly process_heapSize: (a: number, b: number) => [number, number, number];
    readonly process_importRegistry: (a: number, b: number, c: number) => [number, number];
    readonly process_imports: (a: number) => [number, number];
    readonly process_installModules: (a: number, b: number, c: number) => [number, number];
    readonly process_keyInput: (a: number, b: number, c: number, d: number) => [number, number];
    readonly process_map: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly process_mixAudio: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly process_mouseInput: (a: number, b: number, c: number, d: number) => [number, number];
    readonly process_new: (a: any, b: number, c: number) => [number, number, number];
    readonly process_nextMediaTimer: (a: number, b: number) => [number, number, number, number];
    readonly process_read: (a: number, b: number, c: number) => [number, number, number, number];
    readonly process_reserve: (a: number, b: number, c: number) => [number, number];
    readonly process_resolve: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly process_restoreJournal: (a: number, b: number, c: number) => [number, number, number];
    readonly process_resumeCall: (a: number, b: number, c: number) => [number, number, number, number];
    readonly process_resumeException: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly process_scheduleCall: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly process_scheduleCallSmall: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number];
    readonly process_setCommandLine: (a: number, b: number) => [number, number];
    readonly process_setThread: (a: number, b: number) => [number, number];
    readonly process_takeFileChanges: (a: number) => [number, number];
    readonly process_takeFrame: (a: number) => number;
    readonly process_takePackedFrame: (a: number) => number;
    readonly process_takeReadyWaits: (a: number) => [number, number];
    readonly process_takeRegistryChanges: (a: number) => [number, number, number, number];
    readonly process_takeRunnableThreads: (a: number) => [number, number];
    readonly process_takeSyncCallError: (a: number) => any;
    readonly process_virtualAlloc: (a: number, b: number, c: number) => [number, number, number];
    readonly process_virtualFree: (a: number, b: number) => [number, number];
    readonly process_write: (a: number, b: number, c: number, d: number) => [number, number];
    readonly stereoblock_left: (a: number) => any;
    readonly stereoblock_right: (a: number) => any;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
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
