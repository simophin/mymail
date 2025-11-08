import {Observable, Subscription} from "rxjs";
import {createEffect, createSignal, For, JSX, onCleanup, Signal, splitProps} from "solid-js";
import {createStore, Store} from "solid-js/store";
import {binarySearchBy} from "./binarySearch";
import {setEquals} from "./sets";

type PageSubscription = {
    sub: Subscription;
    offset: number;
    limit: number;
}

export type Page<T> = T[];

export type Props<T> = {
    numPerPage: number,
    watchPage: (offset: number, limit: number) => Observable<T[]>,
    children: (item: T | null) => JSX.Element,
    watchingPages?: Signal<Set<number>>,
    pages?: ReturnType<typeof createStore<Page<T>[]>>,
} & Pick<JSX.HTMLAttributes<HTMLElement>, "class"> & Pick<JSX.HTMLAttributes<HTMLElement>, "style">;

function getOffsetBottom(node: HTMLElement): number {
    return node.offsetTop + node.clientHeight
}

function findVisibleChildren(container: HTMLElement): { topChildIndex: number, bottomChildIndex: number } | undefined {
    const scrollY = container.scrollTop;
    const containerHeight = container.clientHeight;

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

type PageSubscriptionMap = Map<number, PageSubscription>;

function ensureSubscriptions(
    [pageSubs, setPageSubs]: Signal<PageSubscriptionMap>,
    ensurePageIndices: Set<number>,
    shouldRecreate: (sub: PageSubscription, pageIndex: number) => boolean,
    createPageSubscription: (pageIndex: number) => PageSubscription) {
    let updatedSubscriptions: PageSubscriptionMap | null = null;

    const ensureUpdatedSubscriptions = () => {
        if (updatedSubscriptions == null) {
            updatedSubscriptions = new Map(pageSubs());
        }

        return updatedSubscriptions;
    };

    for (const pageIndex of ensurePageIndices) {
        const existingSub = pageSubs().get(pageIndex);
        if (!existingSub || shouldRecreate(existingSub, pageIndex)) {
            if (existingSub == null) {
                console.log("Creating subscription for page", pageIndex);
            } else {
                console.log("Updating subscription for page", pageIndex);
            }

            existingSub?.sub?.unsubscribe();
            ensureUpdatedSubscriptions().set(pageIndex, createPageSubscription(pageIndex));
        }
    }

    // See if there's any subscriptions to remove
    for (const [pageIndex, sub] of pageSubs()) {
        if (!ensurePageIndices.has(pageIndex)) {
            console.log("Removing subscription for page", pageIndex);
            ensureUpdatedSubscriptions().delete(pageIndex);
            sub.sub.unsubscribe();
        }
    }

    if (updatedSubscriptions !== null) {
        setPageSubs(updatedSubscriptions);
    }
}

function removePageSubscriptions(
    [pageSubs, setPageSubs]: Signal<PageSubscriptionMap>,
    predicate: (pageIndex: number) => boolean,
) {
    let updatedSubscriptions: PageSubscriptionMap | null = null;

    for (const [pageIndex, sub] of pageSubs()) {
        if (predicate(pageIndex)) {
            if (updatedSubscriptions == null) {
                updatedSubscriptions = new Map(pageSubs());
            }

            sub.sub.unsubscribe();
            updatedSubscriptions.delete(pageIndex);
        }
    }

    if (updatedSubscriptions !== null) {
        setPageSubs(updatedSubscriptions);
    }
}

export default function LazyLoadingList<T>(props: Props<T>) {
    const [localProps, containerProps] = splitProps(props, ["numPerPage", "watchPage", "children"]);

    const [pages, setPages] = props.pages ?? createStore<Page<T>[]>([]);
    const [watchingPages, setWatchingPages] = props.watchingPages ?? createSignal<Set<number>>(new Set([0]));
    const pageSubscriptions = createSignal<PageSubscriptionMap>(new Map());

    const handleContainerEvent = (element: HTMLElement) => {
        const findResult = findVisibleChildren(element);
        if (!findResult) {
            return;
        }

        const { topChildIndex, bottomChildIndex } = findResult;
        const newWatchingPages = new Set<number>();

        const firstVisiblePage = Math.floor(topChildIndex / localProps.numPerPage);
        newWatchingPages.add(firstVisiblePage);

        const lastVisiblePage = Math.floor(bottomChildIndex / localProps.numPerPage);
        newWatchingPages.add(lastVisiblePage);
        newWatchingPages.add(lastVisiblePage + 1);

        if (firstVisiblePage > 0) {
            newWatchingPages.add(firstVisiblePage - 1);
        }

        if (!setEquals(watchingPages(), newWatchingPages)) {
            setWatchingPages(newWatchingPages);
        }
    };

    createEffect(() => {
        ensureSubscriptions(
            pageSubscriptions,
            watchingPages(),
            (sub, pageIndex) => {
                const offset = pageIndex * localProps.numPerPage;
                const limit = localProps.numPerPage;
                return sub.offset !== offset || sub.limit !== limit;
            },
            (pageIndex) => {
                const offset = pageIndex * localProps.numPerPage;
                const limit = localProps.numPerPage;

                return {
                    offset,
                    limit,
                    sub: localProps.watchPage(offset, limit)
                        .subscribe((page) => {
                            const isLastPage = page.length < limit;
                            setPages((oldPages) => {
                                let newPages;
                                if (isLastPage) {
                                    // Remove any pages after this one
                                    removePageSubscriptions(pageSubscriptions, (idx) => idx > pageIndex);
                                    newPages = oldPages.slice(0, pageIndex + 1);
                                } else {
                                    newPages = [...oldPages];
                                }
                                newPages[pageIndex] = page;
                                return newPages;
                            })
                        }),
                }
            }
        );
    })

    onCleanup(() => {
        const [sub, setSub] = pageSubscriptions;
        for (const [_, s] of sub()) {
            s.sub.unsubscribe();
        }

        setSub(new Map());
    });

    let containerRef: HTMLDivElement | undefined;

    createEffect(() => {
        if (!containerRef) return;

        const observer = new ResizeObserver(() => {
            handleContainerEvent(containerRef)
        });

        observer.observe(containerRef);
        onCleanup(() => observer.unobserve(containerRef!));
    });

    return <div {...containerProps}
                onScroll={(d) => handleContainerEvent(d.currentTarget as HTMLElement)} ref={containerRef}>
        <For each={pages}>
            {(page) => (
                <For each={page ?? Array(localProps.numPerPage).fill(null) }>
                    {(item: T | null) => (
                        (localProps.children)(item)
                    )}
                </For>
            )}
        </For>
    </div>
}