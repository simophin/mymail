import {createSignal, onMount, untrack} from "solid-js";


export function AdjustableHorizontalDivider(props: {
    thickness?: number,
    size: number,
    onSizeChange: (newSize: number) => void,
    onDragStarted?: () => void,
    onDragEnded?: () => void,
    class?: string,
}) {
    const [isDragging, setIsDragging] = createSignal(false);

    return <div
        class={`${props.class} h-full w-1 ${isDragging() ? 'bg-accent' : 'bg-base-300/50'} cursor-w-resize`}
        onMouseDown={(e) => {
            e.preventDefault();

            const startX = e.clientX;
            const startSize = untrack(() => props.size);
            props.onDragStarted?.();
            setIsDragging(true);

            function onMouseMove(e: MouseEvent) {
                props.onSizeChange(startSize + (e.clientX - startX));
            }

            function onMouseUp(e: MouseEvent) {
                window.removeEventListener('mousemove', onMouseMove);
                window.removeEventListener('mouseup', onMouseUp);

                props.onDragEnded?.();
                setIsDragging(false);
            }

            window.addEventListener('mousemove', onMouseMove);
            window.addEventListener('mouseup', onMouseUp);
        }}/>
}