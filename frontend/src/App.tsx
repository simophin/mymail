import type {Component} from 'solid-js';
import LazyLoadingList from "./LazyLoadingList";
import {of} from "rxjs";

function* generateItems(offset: number, limit: number) {
    for (let i = offset; i < offset + limit; i++) {
        yield `Item ${i + 1}`;
    }
}

const App: Component = () => {
    return (
        <LazyLoadingList
            numPerPage={30}
            class="h-screen w-screen overflow-y-scroll"
            watchPage={(offset, limit) => of(generateItems(offset, limit).toArray())}>
            {(item) => <div>{item}</div>}
        </LazyLoadingList>
    );
};

export default App;
