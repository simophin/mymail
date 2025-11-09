import {Component, createSignal, Show} from 'solid-js';
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
                class="w-1/5 h-full overflow-y-scroll"/>

            <Show when={selectedMailboxId()}>
                <ThreadList
                    query={{accountId: 1, mailboxId: selectedMailboxId()!}}
                    class="h-screen flex-1 overflow-y-scroll"/>
            </Show>

        </div>
    );
};

export default App;
