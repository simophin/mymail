import type {Component} from 'solid-js';
import ThreadList from "./ThreadList";


const App: Component = () => {
    return (
        <ThreadList
            numPerPage={150}
            query={{accountId: 1, mailboxId: "a"}}
            class="h-screen w-screen overflow-y-scroll" />
    );
};

export default App;
