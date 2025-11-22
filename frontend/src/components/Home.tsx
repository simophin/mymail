import {Component, createMemo, createSignal, Show} from 'solid-js';
import ThreadList, {Thread} from "./ThreadList";
import MailboxList, {MailboxSchema} from "./MailboxList";
import {useNavigate, useSearchParams} from "@solidjs/router";
import HamburgerIcon from "heroicons/24/outline/bars-3.svg";
import ArrowLeftIcon from "heroicons/24/outline/chevron-left.svg";
import {createSignalFromObservableNoError} from "../observables";
import { streamWebSocketApi } from "../streamApi";
import * as zod from "zod";
import {List as ImmutableList} from "immutable";
import {Page} from "./LazyLoadingList";
import ThreadDetails from "./ThreadDetails";

const apiUrl: string = import.meta.env.VITE_BASE_URL;


export type SearchParams = {
    mailboxId?: string;
    threadId?: string;
}

export default function Home() {
    const navigator = useNavigate();
    const [props] = useSearchParams<SearchParams>();

    const drawerToggle = <input id="my-drawer" type="checkbox" class="drawer-toggle"/> as HTMLInputElement;

    const mailboxes = createSignalFromObservableNoError(() => streamWebSocketApi(
        `${apiUrl}/mailboxes/1`,
        zod.array(MailboxSchema),
    ), {
        state: "connecting",
    });

    const threadListPages = createSignal(ImmutableList<Page<Thread>>());

    const selectedMailbox = createMemo(() => {
        const list = mailboxes().lastValue;
        if (!list) {
            return undefined;
        }

        if (props.mailboxId) {
            return list.find(mb => mb.id === props.mailboxId);
        }

        return list.find(mb => mb.role == "inbox");
    });

    const selectedThread = createMemo(() => {
        for (const page of (threadListPages[0])()) {
            for (const thread of page) {
                if (thread.id === props.threadId) {
                    return thread;
                }
            }
        }
    });

    return (
        <div class="drawer md:drawer-open overflow-hidden w-screen h-screen">
            {drawerToggle}
            <div class="drawer-side">
                <label for="my-drawer" class="drawer-overlay" aria-label="close sidebar"/>
                <MailboxList
                    selectedMailboxId={selectedMailbox()?.id}
                    mailboxes={mailboxes().lastValue ?? []}
                    onMailboxSelected={(id) => {
                        navigator(`/?mailboxId=${id}`, { replace: true });
                        drawerToggle.checked = !drawerToggle.checked;
                    }}
                    accountId="1"
                    class="menu md:w-60 bg-base-200 h-full overflow-auto flex flex-col flex-nowrap"/>
            </div>


            <div class="drawer-content md:flex relative w-full h-full overflow-hidden">
                <Show when={!!selectedMailbox()} fallback={"Select a mailbox"}>
                    <div class="h-full w-full md:w-80 absolute md:static overflow-hidden flex flex-col">
                        <div class="flex-none navbar bg-base-100 shadow-sm md:hidden">
                            <label for="my-drawer" class="flex-none btn btn-ghost">
                                <HamburgerIcon class="size-4" />
                            </label>

                            <div class="flex-1">
                                <a class="btn btn-ghost text-xl">{selectedMailbox()?.name}</a>
                            </div>
                        </div>
                        <ThreadList
                            selectedThreadId={props.threadId}
                            pages={threadListPages}
                            onThreadSelected={(id) => {
                                navigator(`/?mailboxId=${selectedMailbox()!.id}&threadId=${id}`, { replace: !!props.threadId });
                            }}
                            class="flex-1"
                            query={{accountId: 1, mailboxId: selectedMailbox()!.id}}/>
                    </div>
                </Show>

                <Show when={!!selectedThread()}>
                    <div class="h-full w-full md:flex-1 absolute md:static overflow-hidden flex flex-col bg-base-100">
                        <div class="flex-none navbar bg-base-100 shadow-sm md:hidden">
                            <button class="flex-none btn btn-ghost" onClick={() => navigator(-1)}>
                                <ArrowLeftIcon class="size-4 "/>
                            </button>

                            <div class="flex-1">
                                <a class="btn btn-ghost text-xl">Read mail</a>
                            </div>
                        </div>

                        <ThreadDetails accountId={1} thread={selectedThread()!} class="flex-1 w-full"/>
                    </div>
                </Show>
            </div>

        </div>
    );
};
