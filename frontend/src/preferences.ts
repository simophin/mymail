import * as zod from 'zod';
import {Accessor, createEffect, createSignal, onCleanup, Signal} from "solid-js";
import {log as parentLog} from "./log";

const log = parentLog.child({ "component": "createPreference" });

export function createPreference<T>(key: string | Accessor<string>, type: zod.ZodType<T>, defaultValue: T): Signal<T> {
    const [value, setStoreValue] = createSignal<T>(defaultValue);
    const keyAccess = () => typeof key === 'function' ? key() : key;

    createEffect(() => {
        const stored = localStorage.getItem(keyAccess());
        log.debug(`Stored ${keyAccess()} is ${stored}`);
        if (stored != null) {
            try {
                const value = type.parse(JSON.parse(stored));
                setStoreValue(() => value);
            } catch (e) {

            }
        }
    });

    createEffect(() => {
        const computedKey = keyAccess();
        const computedValue = value();
        const timer = setTimeout(() => localStorage.setItem(computedKey, JSON.stringify(computedValue)), 500);
        onCleanup(() => clearTimeout(timer));
    });

    return [value, setStoreValue];
}