<?php

namespace App;

class Base
{
    protected function boot(): void
    {
    }

    public function init(): void
    {
        $this->boot();
    }
}
