import { createEffect, createSignal, Index } from "solid-js";
import { createStore } from "solid-js/store/types/server.js";
const apiUrl = import.meta.env.BASE_URL;

export type EmailSort = {
    column: string;
    asc: boolean;
}


export type BasicEmailDbQuery = {
    mailboxId?: string;
    searchKeyword?: string;
}

export type EmailDbQuery = BasicEmailDbQuery & {
    sorts: EmailSort[];
    limit: number;
    offset: number;
}

export type AccountId = number;

export type Email = {
    id: string;
    subject: string;
}

export default function MailList(props: {
    accountId: AccountId;
    query: BasicEmailDbQuery
}) {
    const [pageSize, setPageSize] = createSignal(50);
    const [pageIndex, setPageIndex] = createSignal(0);
    const [pages, setPages] = createStore<Email[][]>([]);

    createEffect<AbortController | undefined>((prev) => {
        prev?.abort();

        const controller = new AbortController();
        async function fetchMails() {
            const res = await fetch(`${apiUrl}/mails/${props.accountId}`, {
                method: 'POST',
                body: JSON.stringify({
                    ...props.query,
                    limit: pageSize(),
                    offset: pageIndex() * pageSize(),
                })
            });

            const reader = res.body!.getReader();
            const decoder = new TextDecoder();

            while (!controller.signal.aborted) {
                const { done, value } = await reader.read();
                if (done) break;

                setPages((pages) => {
                    return [
                        ...pages.slice(0, pageIndex()),
                        JSON.parse(decoder.decode(value)),
                        ...pages.slice(pageIndex() + 1)
                    ]
                })
            }
        };

        fetchMails();

        return controller;
    });

    
}