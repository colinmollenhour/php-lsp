<?php

namespace App;

class Noise24
{
    private function compute(): int
    {
        return 24;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
