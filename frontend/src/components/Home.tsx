import {Component, createSignal, Show} from 'solid-js';
import ThreadList from "./ThreadList";
import MailboxList from "./MailboxList";
import {useNavigate, useSearchParams} from "@solidjs/router";

export type SearchParams = {
    mailboxId?: string;
    threadId?: string;
}

export default function Home() {
    const navigator = useNavigate();
    const [props] = useSearchParams<SearchParams>();

    const drawerToggle = <input id="my-drawer" type="checkbox" class="drawer-toggle"/> as HTMLInputElement;

    return (
        <div class="drawer md:drawer-open overflow-hidden">
            {drawerToggle}
            <div class="drawer-side">
                <label for="my-drawer" class="drawer-overlay" aria-label="close sidebar"/>
                <MailboxList
                    selectedMailboxId={props.mailboxId}
                    onMailboxSelected={(id) => {
                        navigator(`/?mailboxId=${id}`);
                        drawerToggle.checked = !drawerToggle.checked;
                    }}
                    accountId="1"
                    class="menu w-60 h-full overflow-auto flex flex-col"/>
            </div>


            <div class="drawer-content flex h-screen">
                <label for="my-drawer" class="btn drawer-button md:hidden">
                    Open drawer
                </label>

                <Show when={!!props.mailboxId} fallback={"Select a mailbox"}>
                    <div class="h-full w-80 overflow-hidden">
                        <ThreadList
                            selectedThreadId={props.threadId}
                            onThreadSelected={(id) => {
                                navigator(`/?mailboxId=${props.mailboxId}&threadId=${id}`);
                            }}
                            query={{accountId: 1, mailboxId: props.mailboxId!}}/>
                    </div>
                </Show>
            </div>

        </div>
    );
};
