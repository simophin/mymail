import LazyLoadingList, {Page, Props as ListProps} from "./LazyLoadingList";
import {createEffect, createSignal, onCleanup, splitProps, untrack} from "solid-js";
import {streamWebSocketApi} from "./streamApi";
import {BehaviorSubject, filter, map, retry} from "rxjs";
import {List as ImmutableList, Set as ImmutableSet} from "immutable";
import * as zod from "zod";

import {log as parentLog} from "./log";

const log = parentLog.child({"component": "ThreadList"});

const apiUrl: string = import.meta.env.VITE_BASE_URL;

export type EmailSort = {
    column: 'Date';
    asc: boolean;
}

export type ThreadQuery = {
    accountId: AccountId,
    mailboxId: string;
}

export type AccountId = number;

const EmailSchema = zod.object({
    id: zod.string(),
    subject: zod.string(),
    receivedAt: zod.string(),
});

const ThreadSchema = zod.object({
    id: zod.string(),
    emails: zod.array(EmailSchema),
});

export type Email = zod.infer<typeof EmailSchema>;
export type Thread = zod.infer<typeof ThreadSchema>;

type EmailQuery = {
    anchor_id?: string,
    mailbox_id?: string,
    search_keyword?: string,
    sorts: EmailSort[],
    limit: number,
};
type EmailSyncState = {};

export default function ThreadList(props: {
    query: ThreadQuery,
} & Pick<ListProps<Thread>, "style"> & Pick<ListProps<Thread>, "class">) {
    const [localProps, listProps] = splitProps(props, ["query"]);
    const watchingPages = createSignal(ImmutableSet([0]));
    const watchPage = (offset: number, limit: number) => {
        return streamWebSocketApi(
            `${apiUrl}/threads/${localProps.query.accountId}?mailbox_id=${localProps.query.mailboxId}&offset=${offset}&limit=${limit}`,
            zod.array(ThreadSchema)
        )
            .pipe(
                retry({ count: Infinity, delay: 1000 }),
                filter((v) => !!v.lastValue),
                map((v) => v.lastValue!)
            )
    };
    const pages = createSignal(ImmutableList<Page<Thread>>([]));
    const numPerPage = 100;

    const emailSyncQuery = new BehaviorSubject<EmailQuery | undefined>(undefined)

    createEffect(() => {
        const sub = streamWebSocketApi<EmailSyncState>(`${apiUrl}/mailboxes/sync/${localProps.query.accountId}/${localProps.query.mailboxId}`,
            zod.object(),
            emailSyncQuery)
            .pipe(retry({ count: Infinity, delay: 1000 }))
            .subscribe((syncState) => {
                log.info({syncState}, "Got new email sync state");
            });

        onCleanup(() => sub.unsubscribe());
        return sub;
    });

    createEffect(() => {
        let minPage: number | undefined, maxPage: number | undefined;
        (watchingPages[0])().forEach((page) => {
            if (typeof minPage == "undefined" || page < minPage) {
                minPage = page;
            }

            if (typeof maxPage == "undefined" || page > maxPage) {
                maxPage = page;
            }
        });

        if (minPage == undefined || maxPage == undefined) {
            return;
        }

        let anchor_id: string | undefined;
        let limit: number;
        const p = untrack(() => (pages[0])().toArray());
        anchor_id = last(p.at(minPage)?.at(0)?.emails)?.id;

        log.info({
            from: minPage * numPerPage,
            to: (maxPage + 1) * numPerPage,
        }, "Watching offset");
        emailSyncQuery.next({
            anchor_id,
            mailbox_id: localProps.query.mailboxId,
            sorts: [{ column: 'Date', asc: false }],
            limit: (maxPage - minPage + 1) * numPerPage * 10,
        })
    });

    return <LazyLoadingList
        {...listProps}
        numPerPage={numPerPage}
        watchPage={watchPage}
        class="list h-full overflow-y-scroll"
        pages={pages}
        deps={[localProps.query.accountId, localProps.query.mailboxId]}
        watchingPages={watchingPages}>
        {(thread) => (
            <div class="list-row cursor-pointer hover:bg-base-200" role="link">
                {thread?.emails?.at(0)?.subject}
            </div>
        )}
    </LazyLoadingList>;
}

function last<T>(arr: T[] | undefined): T | undefined {
    if (!arr || arr.length === 0) {
        return undefined;
    }

    return arr[arr.length - 1];
}