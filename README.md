# YouTube RSS Downloader

Takes an array of RSS channel feeds and downloads all the latest videos to the specified location. 

Uses [yt-dlp](https://github.com/yt-dlp/yt-dlp) to do the actual downloading.

Works using `--download-archive` mode which maintains a list of videos that have already been downloaded so we an avoid re-downloading anything.

I find the RSS feeds to be the most accurate source of what has actually been posted and am tired of flakey YouTube alternative apps/front-ends, so here's my attempt at something more reliable, and much lighter. You get to maintain your own subscription list and use your own video player/file browser!

As an experiment, this code was generated 100% using Chat GPT o3-mini-high.  I worked with ChatGPT to descibe what needs to happen etc.  All the code and comments are ChatGPT's (apart from the example subscription URL I stuck in there).