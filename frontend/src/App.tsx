import { Component, createSignal, Show } from 'solid-js';
import ThreadList from "./ThreadList";
import MailboxList from "./MailboxList";


const App: Component = () => {
    const [selectedMailboxId, setSelectedMailboxId] = createSignal<string>();

    return (
        <div class="h-screen w-screen flex">
            <MailboxList
                selectedMailboxId={selectedMailboxId()}
                onMailboxSelected={setSelectedMailboxId}
                accountId="1"
                class="menu w-1/6 h-full overflow-auto flex flex-col" />


            <Show when={selectedMailboxId()}>
                <div class="h-full w-1/4 overflow-hidden">
                <ThreadList
                    query={{ accountId: 1, mailboxId: selectedMailboxId()! }} />
                </div>
            </Show>

        </div>
    );
};

export default App;
