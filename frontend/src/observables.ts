import * as rx from "rxjs";
import {Accessor, createEffect, createSignal, onCleanup} from "solid-js";

export type ObservableState<T> = {} | {
    value: T
} | {
    error: any
}

export function createSignalFromObservable<T>(factory: () => rx.Observable<T>): Accessor<ObservableState<T>> {
    const [state, setState] = createSignal<ObservableState<T>>({});

    createEffect(() => {
        const observable = factory();
        const subscription = observable.subscribe({
            next: (value) => {
                setState({ value });
            },
            error: (error) => {
                setState({ error });
            }
        });

        onCleanup(() => subscription.unsubscribe());
    });

    return state;
}

export function createSignalFromObservableNoError<T>(factory: () => rx.Observable<T>, initialValue: T): Accessor<T> {
    const [value, setValue] = createSignal<T>(initialValue);

    createEffect(() => {
        const observable = factory();
        const subscription = observable.subscribe((value) => {
            setValue(() => value);
        });

        onCleanup(() => subscription.unsubscribe());
    });

    return value;
}