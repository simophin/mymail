import {Component, createSignal, Show} from 'solid-js';
import ThreadList from "./ThreadList";
import MailboxList, {MailboxSchema} from "./MailboxList";
import {useNavigate, useSearchParams} from "@solidjs/router";
import HamburgerIcon from "heroicons/24/outline/bars-3.svg?raw";
import {createSignalFromObservableNoError} from "../observables";
import { streamWebSocketApi } from "../streamApi";
import * as zod from "zod";

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

    const selectedMailboxName = () => {
        const mailbox = mailboxes().lastValue?.find(mb => mb.id === props.mailboxId);
        return mailbox?.name;
    }

    return (
        <div class="drawer md:drawer-open overflow-hidden w-screen h-screen">
            {drawerToggle}
            <div class="drawer-side">
                <label for="my-drawer" class="drawer-overlay" aria-label="close sidebar"/>
                <MailboxList
                    selectedMailboxId={props.mailboxId}
                    mailboxes={mailboxes().lastValue ?? []}
                    onMailboxSelected={(id) => {
                        navigator(`/?mailboxId=${id}`, { replace: true });
                        drawerToggle.checked = !drawerToggle.checked;
                    }}
                    accountId="1"
                    class="menu w-60 h-full overflow-auto flex flex-col flex-nowrap"/>
            </div>


            <div class="drawer-content md:flex relative w-full h-full overflow-hidden">
                <Show when={!!props.mailboxId} fallback={"Select a mailbox"}>
                    <div class="h-full w-full md:w-80 absolute md:static overflow-hidden flex flex-col">
                        <div class="flex-none navbar bg-base-100 shadow-sm md:hidden">
                            <label for="my-drawer" class="flex-none btn btn-ghost">
                                <span class="size-4" innerHTML={HamburgerIcon} />
                            </label>

                            <div class="flex-1">
                                <a class="btn btn-ghost text-xl">{selectedMailboxName()}</a>
                            </div>
                        </div>
                        <ThreadList
                            selectedThreadId={props.threadId}
                            onThreadSelected={(id) => {
                                navigator(`/?mailboxId=${props.mailboxId}&threadId=${id}`, { replace: true });
                            }}
                            class="flex-1"
                            query={{accountId: 1, mailboxId: props.mailboxId!}}/>
                    </div>
                </Show>
            </div>

        </div>
    );
};
