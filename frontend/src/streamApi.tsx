import {map, Observable, repeat, scan, Subject, Subscription} from "rxjs";
import {Accessor, createEffect, createSignal, onCleanup, Signal} from "solid-js";

export type WebSocketState<T> = {
    state: "connecting" | "open" | "closed",
    lastValue?: T,
} | {
    state: "error",
    lastValue?: T,
    error: any,
};

export function streamWebSocketApi<T, S = any>(url: string, messageToSend?: Subject<S | undefined>) {
    return new Observable<WebSocketState<T>>((subscriber) => {
        const ws = new WebSocket(url);

        subscriber.next({
            state: "connecting",
        });

        ws.onmessage = (event) => {
            let value = JSON.parse(event.data);
            subscriber.next({
                state: "open",
                lastValue: value,
            });
        };

        ws.onerror = (event) => {
            subscriber.next({
                state: "error",
                error: event.type,
            })
            subscriber.complete();
        };

        const subscriptions: Subscription[] = [];

        ws.onopen = () => {
            subscriber.next({"state": "open"});

            if (messageToSend) {
                subscriptions.push(messageToSend.subscribe((message) => {
                    if (typeof message !== "undefined") {
                        ws.send(JSON.stringify(message));
                    }
                }));
            }
        };

        ws.onclose = () => {
            subscriber.complete();
        };

        return () => {
            for (const subscription of subscriptions) {
                subscription.unsubscribe();
            }
            ws.close();
        };
    }).pipe(
        repeat({
            delay: 1000,
            count: Infinity,
        }),

        scan((acc, curr) => ({
            ...curr,
            lastValue: curr.lastValue ?? acc.lastValue,
        })),
    );
}


export function createWebSocketResource<T>(initial: T, factory: () => WebSocket) {
    const [data, setData] = createSignal<T>(initial);
    const [reloadSeq, setReloadSeq] = createSignal(0);

    createEffect(() => {
        const ws = factory();
        let _ = reloadSeq();

        ws.onmessage = (event) => {
            setData(JSON.parse(event.data));
        };

        ws.onerror = (event) => {
            console.error("WebSocket error", event);
            setTimeout(() => setReloadSeq((n) => n + 1), 1000);
        };

        onCleanup(() => ws.close());
    });

    return data;
}