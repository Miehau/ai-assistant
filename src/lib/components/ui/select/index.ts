import { Select as SelectPrimitive } from "bits-ui";

import Item from "./select-item.svelte";
import Content from "./select-content.svelte";
import Trigger from "./select-trigger.svelte";

const Root = SelectPrimitive.Root;
const Group = SelectPrimitive.Group;
const GroupHeading = SelectPrimitive.GroupHeading;
const Portal = SelectPrimitive.Portal;
const Viewport = SelectPrimitive.Viewport;
const ScrollUpButton = SelectPrimitive.ScrollUpButton;
const ScrollDownButton = SelectPrimitive.ScrollDownButton;

export {
  Root,
  Item,
  Group,
  GroupHeading,
  Content,
  Trigger,
  Viewport,
  ScrollUpButton,
  ScrollDownButton,
  Portal,
  //
  Root as Select,
  Item as SelectItem,
  Group as SelectGroup,
  GroupHeading as SelectGroupHeading,
  Content as SelectContent,
  Trigger as SelectTrigger,
  Viewport as SelectViewport,
  ScrollUpButton as SelectScrollUpButton,
  ScrollDownButton as SelectScrollDownButton,
  Portal as SelectPortal,
};
