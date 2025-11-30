import * as zod from 'zod';
import {Accessor, createDeferred, createEffect, createSignal, Setter, Signal, untrack} from "solid-js";

export function createPreference<T>(key: string | Accessor<string>, type: zod.ZodType<T>, defaultValue: T): Signal<T> {
    const [value, setStoreValue] = createSignal<T>(defaultValue);

    createEffect(() => {
        const stored = localStorage.getItem(typeof key === 'function' ? key() : key);
        const value = type.parse(stored ? JSON.parse(stored) : null);
        setStoreValue(() => value);
    });

    createDeferred(() => {

    });

    const valueSetter: Setter<T>;

    return [value, (newValue: T | ((oldValue: T) => T)) => {
        setStoreValue(() => newValue);
        localStorage.setItem(typeof key === 'function' ? untrack(key) : key, JSON.stringify(newValue));
    }];
}