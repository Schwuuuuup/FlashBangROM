## List of known bugs in FlashBang Studio. This is not an exhaustive list, but it should give you an idea of some of the issues that are currently being worked on.

# GUI: SerialMonitor Layout:
The SerialMonitor does not adjust correctly to the window, it might be higher than the available space pushing the progress bar out of frame. I also might be smaler than the GUI panel, not filling it and not beeing centered - thus looking broken

# GUI: Transferbuttons not centered
It is the intend to have the area between the Inspector and the Workbench smack down in the middle, leaving equal width for both ofthemn... this is not the case. The layout should be:
MMMMMMMMMMMMM
IIIII T WWWWW
IIIII T WWWWW
IIIII T WWWWW
IIIII T WWWWW
CCCCC T DDDDD
-------------
SSSSSSSSSSSSS
SSSSSSSSSSSSS
SSSSSSSSSSSSS
PPPPPPPPPPPPP

M = Menu/Info-Area (can wrap)
I = Inspector (scrollable in sync with Workbench)
T = Transferbuttons
W = Workbench (scrollable in sync with Inspector)
C = ChipButtons (can wrap)
D = DiskButtons (can wrap)
S = SerialMonitor (Scrollable)
P = Progressbar (might be hidden if not active (optional feature))
- = Adjustable divider

So when the Window gets wider and the divider is mooved, you might get:

MMMMMMMMMMMMMMMMMMM
IIIIIIII T WWWWWWWW
IIIIIIII T WWWWWWWW
IIIIIIII T WWWWWWWW
IIIIIIII T WWWWWWWW
CCCCCCCC T DDDDDDDD
-------------------
SSSSSSSSSSSSSSSSSSS
PPPPPPPPPPPPPPPPPPP

And On Narrow windows you might get (M, C and D are now wrapping to multiple lines)

MMMMMMMMM
MMMMMMMMM
III T WWW
III T WWW
III T WWW
III T WWW
CCC T DDD
CCC T DDD
---------
SSSSSSSSS
SSSSSSSSS
SSSSSSSSS
PPPPPPPPP


# Driver Selection:
If there are multiple drivers for one Chip-ID you still get awarning having the "wrong" driver selected if a matching driver ist selected, that is not the default one.

# Protocol (or atleast the client-Side interpretation)
 The differentiation between "built-in" commands (all Caps) and custom commands (all lowercase) is not strictly impelemented. The built-in commands should be the minimal set, that barely let you work with a chip. "sector erase" of the SST39 Chips is a built-in command, but it does not exist on Winbond chips. It Can be implemented though by writing a single "FF" into any byte of a 128 byte sector, but Inkremental writing of ranges that are not alligned with sector boundaries is not possible. "sector erase" should be a custom command. While "program byte" might be implemented as a built-in internal command, but should not be made available (or be disabled in the driver) for certain chips that do not support it, like the Winbond chips. The Application should "understand" that flashing  a "Range" is just a bunchh of single "flash byte" commands.

 # Event Handling and Timeouts:
 Long running operations like "Flash Image" do not hit a hard timeout, but it seems the GUI does not correctly handle the ending-event (if there is one). the progresscounter stops at the last buyte (effectivly 100%) but no autofetch is trigered, and the Inspector does not go into ther "gray" unknown data state either.. it just looks like it is still working, but it is finished. the Gui does work and manually hitting the "fetch" button does fetch the data and update the inspector, but it should be automatic. Also, if there is a timeout, it should be handled gracefully, showing an error message and allowing the user to retry or cancel.

 # Responsiveness:
 the gui ist sluggish at best, i did not do any analysis yet,but memory usage and/or too many draw calls might be to blame - especially the preview window and the PNG Import seem to be hart to the cpu. Als hany type of scrolling is really slow.

 # Serial Monitor: collapsable output is needed:
  When flashing an image, the serial monitor gets flooded with "STATUS|FLASH_IMAGE|BYTE_PROGRAM|X|Y" messages, which are useful for debugging, but not for the user. It would be nice to have a collapsable section in the serial monitor that can hide these messages, or at least group them together.