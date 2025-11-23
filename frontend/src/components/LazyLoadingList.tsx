import {Observable, Subscription} from "rxjs";
import {createEffect, createSignal, For, JSX, onCleanup, Signal, splitProps, untrack} from "solid-js";
import {binarySearchBy} from "../binarySearch";
import {Map as ImmutableMap, Set as ImmutableSet, List as ImmutableList} from "immutable";
import {log as parentLog} from "../log";

const log = parentLog.child({"component": "LazyLoadingList"});

type PageSubscription = {
    sub: Subscription;
    deps: ImmutableList<any>;
}

export type Page<T> = T[];

export type Props<T> = {
    numPerPage: number,
    watchPage: (offset: number, limit: number) => Observable<T[]>,
    children: (item: T | null) => JSX.Element,
    watchingPages?: Signal<ImmutableSet<number>>,
    pages?: Signal<ImmutableList<Page<T>>>,
    deps?: any[],
    class?: string,
    jumpToHeadTimestamp?: number,
};

export default function LazyLoadingList<T>(props: Props<T>) {
    const [localProps, containerProps] = splitProps(props, ["numPerPage", "watchPage", "children", "pages", "watchingPages", "deps"]);

    // Final page data to render
    const [pages, setPages] = props.pages ?? createSignal(ImmutableList<Page<T>>([]));

    // Currently watched page indices
    const [watchingPages, setWatchingPages] = props.watchingPages ?? createSignal(ImmutableSet([0]));

    // Subscriptions for each watched page
    let pageSubs = ImmutableMap<number, PageSubscription>()

    const handleContainerEvent = (element: HTMLElement) => {
        const findResult = findVisibleChildren(element);
        if (!findResult) {
            return;
        }

        const {topChildIndex, bottomChildIndex} = findResult;
        const newWatchingPages: number[] = [];

        const firstVisiblePage = Math.floor(topChildIndex / localProps.numPerPage);
        newWatchingPages.push(firstVisiblePage);

        const lastVisiblePage = Math.floor(bottomChildIndex / localProps.numPerPage);
        newWatchingPages.push(lastVisiblePage);
        newWatchingPages.push(lastVisiblePage + 1);

        if (firstVisiblePage > 0) {
            newWatchingPages.push(firstVisiblePage - 1);
        }

        const newWatchingPagesSet = ImmutableSet(newWatchingPages);

        if (!watchingPages().equals(newWatchingPagesSet)) {
            setWatchingPages(newWatchingPagesSet);
        }
    };

    createEffect(() => {
        // Find out the updated watched pages
        for (const pageIdx of watchingPages()) {
            const offset = pageIdx * localProps.numPerPage;
            const limit = localProps.numPerPage;
            const newDeps = ImmutableList(
                [...(localProps.deps ?? []), offset, limit]
            );

            const sub = pageSubs.get(pageIdx);

            if (sub && sub.deps.equals(newDeps)) {
                continue;
            }

            log.info({pageIdx}, "Subscribing to page");

            if (sub != null) {
                sub.sub.unsubscribe();
                setPages((oldPages) => oldPages.set(pageIdx, []));
            }

            sub?.sub?.unsubscribe();
            pageSubs = pageSubs.set(pageIdx, {
                deps: newDeps,
                sub: untrack(() => localProps.watchPage(offset, limit))
                    .subscribe((page) => {
                        log.info({pageIdx, size: page.length}, "Received data for page");
                        let newPages = pages().set(pageIdx, page);
                        if (page.length < limit) {
                            // If we received fewer items than requested, we can assume this is the last page
                            newPages = newPages.slice(0, pageIdx + 1);

                            // Also remove all watched pages beyond this one
                            const newWatchingPages = watchingPages().filter((p) => p <= pageIdx);
                            if (!newWatchingPages.equals(watchingPages())) {
                                setWatchingPages(newWatchingPages);
                            }
                        }

                        if (!newPages.equals(pages())) {
                            setPages(newPages);
                        }
                    })
            });
        }

        // Find out removed pages
        const oldSubs = pageSubs;
        for (const [pageIdx, sub] of oldSubs) {
            if (!watchingPages().has(pageIdx)) {
                sub.sub.unsubscribe();
                pageSubs = pageSubs.delete(pageIdx);
                log.info({pageIdx}, "Unsubscribed page data");
            }
        }
    })

    onCleanup(() => {
        for (const [_, s] of pageSubs) {
            s.sub.unsubscribe();
        }
    });

    const container = <ul {...containerProps}
                           onScroll={(d) => handleContainerEvent(d.currentTarget as HTMLElement)}>
        <For each={pages().toArray()}>
            {(page) => (
                <For each={page ?? Array(localProps.numPerPage).fill(null)}>
                    {(item: T | null) => (
                        (localProps.children)(item)
                    )}
                </For>
            )}
        </For>
    </ul> as HTMLElement;

    createEffect(() => {
        const observer = new ResizeObserver(() => {
            handleContainerEvent(container)
        });

        observer.observe(container);
        onCleanup(() => observer.unobserve(container));
    });

    createEffect(() => {
        if (props.jumpToHeadTimestamp) {
            container.scrollTo({ top: 0});
        }
    });

    return container;
}

function getOffsetBottom(node: HTMLElement): number {
    return node.offsetTop + node.clientHeight
}

function findVisibleChildren(container: HTMLElement): { topChildIndex: number, bottomChildIndex: number } | undefined {
    const scrollY = container.scrollTop;
    const containerHeight = Math.min(container.clientHeight, window.innerHeight);

    if (container.childNodes.length === 0) {
        return;
    }

    // Find first visible child
    const topChildIndex = binarySearchBy(container.childNodes, scrollY, (n) => getOffsetBottom(n as HTMLElement));

    // Find last visible child
    const bottomIndex = binarySearchBy(container.childNodes, scrollY + containerHeight, (n) => (n as HTMLElement).offsetTop, topChildIndex);

    return {
        topChildIndex,
        bottomChildIndex: Math.min(container.childNodes.length - 1, bottomIndex),
    };
}
