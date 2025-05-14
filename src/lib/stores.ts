import { writable } from "svelte/store";
import type { Device } from "./types";

export const pairedDevices = writable<
    Map<string, { device: Device; online: boolean; last_seen: number }>
  >(new Map());