import LazyLoadingList, { Page } from "./LazyLoadingList";
import { createEffect, createSignal, onCleanup, Signal, untrack } from "solid-js";
import { streamWebSocketApi } from "../streamApi";
import { BehaviorSubject, filter, map, retry } from "rxjs";
import { List as ImmutableList, Set as ImmutableSet } from "immutable";
import * as zod from "zod";
import PaperClipIcon from "heroicons/24/outline/paper-clip.svg";

import { log as parentLog } from "../log";
import EmailIcon from "./EmailIcon";
import { formatRelative } from "date-fns";
import {formatShortDateTime} from "../formats";

const log = parentLog.child({ "component": "ThreadList" });

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

const BodyPartSchema = zod.object({
    partId: zod.string(),
    blobId: zod.string(),
    type: zod.string(),
    size: zod.number(),
    cid: zod.string().nullish(),
});

const EmailAddressSchema = zod.object({
    name: zod.string().nullable().optional(),
    email: zod.string().nullable(),
});

const EmailSchema = zod.object({
    id: zod.string(),
    subject: zod.string().optional(),
    receivedAt: zod.string(),
    preview: zod.string().optional(),
    htmlBody: zod.array(BodyPartSchema),
    textBody: zod.array(BodyPartSchema),
    attachments: zod.array(BodyPartSchema).optional(),
    hasAttachment: zod.boolean(),
    from: zod.array(EmailAddressSchema),
});

const ThreadSchema = zod.object({
    id: zod.string(),
    emails: zod.array(EmailSchema),
});

export type Email = zod.infer<typeof EmailSchema>;
export type Thread = zod.infer<typeof ThreadSchema>;
export type BodyPart = zod.infer<typeof BodyPartSchema>;

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
    selectedThreadId?: string,
    onThreadSelected?: (threadId: string) => void,
    class?: string,
    pages?: Signal<ImmutableList<Page<Thread>>>,
    jumpToHeadTimestamp?: number,
}) {
    const watchingPages = createSignal(ImmutableSet([0]));
    const watchPage = (offset: number, limit: number) => {
        return streamWebSocketApi(
            `${apiUrl}/threads/${props.query.accountId}?mailbox_id=${props.query.mailboxId}&offset=${offset}&limit=${limit}`,
            zod.array(ThreadSchema)
        )
            .pipe(
                retry({ count: Infinity, delay: 1000 }),
                filter((v) => !!v.lastValue),
                map((v) => v.lastValue!)
            )
    };
    const pages = props.pages ?? createSignal(ImmutableList<Page<Thread>>([]));
    const numPerPage = 100;

    const emailSyncQuery = new BehaviorSubject<EmailQuery | undefined>(undefined)

    createEffect(() => {
        const sub = streamWebSocketApi<EmailSyncState>(`${apiUrl}/mailboxes/sync/${props.query.accountId}/${props.query.mailboxId}`,
            zod.object(),
            emailSyncQuery)
            .pipe(retry({ count: Infinity, delay: 1000 }))
            .subscribe((syncState) => {
                log.info({ syncState }, "Got new email sync state");
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
        const p = untrack(() => (pages[0])().toArray());
        anchor_id = last(p.at(minPage)?.at(0)?.emails)?.id;

        log.info({
            from: minPage * numPerPage,
            to: (maxPage + 1) * numPerPage,
        }, "Watching offset");
        emailSyncQuery.next({
            anchor_id,
            mailbox_id: props.query.mailboxId,
            sorts: [{ column: 'Date', asc: false }],
            limit: (maxPage - minPage + 1) * numPerPage * 10,
        })
    });

    const onThreadItemClick = (evt: Event) => {
        const ele = evt.currentTarget as HTMLElement;
        log.info({ id: ele.dataset.id }, "Thread item clicked");
        props.onThreadSelected?.(ele.dataset.id!);
    };

    const containsAttachment = (thread: Thread) => {
        return thread.emails.some((email) => email.hasAttachment)
    };

    return <LazyLoadingList
        numPerPage={numPerPage}
        watchPage={watchPage}
        class={`${props.class} list w-full h-full overflow-y-scroll overflow-x-auto`}
        pages={pages}
        deps={[props.query.accountId, props.query.mailboxId]}
        jumpToHeadTimestamp={props.jumpToHeadTimestamp}
        watchingPages={watchingPages}>
        {(thread) => (
            <li class={`hover:bg-base-200 p-4 border-b-base-200 border-b-2 flex items-center cursor-pointer ${(props.selectedThreadId && props.selectedThreadId == thread?.id) ? 'bg-base-200' : ''}`}
                data-id={thread?.id}
                onClick={onThreadItemClick}
                role="link">

                <EmailIcon address={thread!.emails?.at(0)?.from?.at(0)?.email ?? ''}
                    name={thread!.emails?.at(0)?.from?.at(0)?.name}
                    class="flex-none not-prose mr-2 rounded-full"
                    size="lg" />

                <div class="flex-1">
                    <h4 class="flex items-center mb-1">
                        {containsAttachment(thread!) &&
                            <PaperClipIcon class="size-4 inline-block mr-1 align-text-bottom" />}
                        <span class="text-lg flex-1 overflow-x-hidden line-clamp-1 text-ellipsis">{thread?.emails?.at(0)?.subject}</span>
                        <span class="ml-2 text-sm text-gray-500">{formatThreadTime(thread!)}</span>
                    </h4>
                    <div class="line-clamp-2 text-sm">
                        <ThreadPreview thread={thread!} />
                    </div>
                </div>

            </li>
        )}
    </LazyLoadingList>;
}

function formatThreadTime(thread: Thread) {
    const firstMail = thread.emails[0];
    return formatShortDateTime(new Date(firstMail.receivedAt));
}

function ThreadPreview(props: { thread: Thread }) {
    const firstMail = () => props.thread.emails[0];

    const sender = () => {
        const senderName = firstMail().from[0].name ?? firstMail().from[0].email;
        return senderName && <b>{senderName}:&nbsp;</b>;
    }

    return <>
        {sender()}
        {firstMail().subject}
    </>
}

function last<T>(arr: T[] | undefined): T | undefined {
    if (!arr || arr.length === 0) {
        return undefined;
    }

    return arr[arr.length - 1];
}